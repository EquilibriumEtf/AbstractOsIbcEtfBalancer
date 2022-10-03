use std::collections::HashSet;
use std::convert::TryInto;

use abstract_os::objects::{AssetEntry, UncheckedContractEntry};

use cosmwasm_std::{Addr, Deps, Env, Order, StdError, StdResult, Uint128};
use cw_storage_plus::Bound;

use crate::msg::{AssetsResponse, ConfigResponse, ValidityResponse};
use crate::proxy_asset::{get_pair_asset_names, other_asset_name, ProxyAsset, ValueRef};
use crate::state::{MEMORY, STATE, VAULT_ASSETS};
const DEFAULT_LIMIT: u8 = 5;
const MAX_LIMIT: u8 = 20;
pub fn query_proxy_assets(
    deps: Deps,
    last_asset_name: Option<String>,
    limit: Option<u8>,
) -> StdResult<AssetsResponse> {
    let limit = limit.unwrap_or(DEFAULT_LIMIT).min(MAX_LIMIT) as usize;
    let start_bound = last_asset_name.as_deref().map(Bound::exclusive);

    let res: Result<Vec<(AssetEntry, ProxyAsset)>, _> = VAULT_ASSETS
        .range(deps.storage, start_bound, None, Order::Ascending)
        .take(limit)
        .collect();

    let names_and_configs = res?;
    Ok(AssetsResponse {
        assets: names_and_configs,
    })
}

/// Returns the whitelisted modules
pub fn query_config(deps: Deps) -> StdResult<ConfigResponse> {
    let state = STATE.load(deps.storage)?;
    let modules: Vec<Addr> = state.modules;
    let resp = ConfigResponse {
        modules: modules
            .iter()
            .map(|module| -> String { module.to_string() })
            .collect(),
    };
    Ok(resp)
}

/// Returns the value of a specified asset.
pub fn compute_holding_value(deps: Deps, env: &Env, asset_entry: String) -> StdResult<Uint128> {
    let mut vault_asset: ProxyAsset = VAULT_ASSETS.load(deps.storage, asset_entry.into())?;
    let memory = MEMORY.load(deps.storage)?;
    let value = vault_asset.value(deps, env, &memory, None)?;
    Ok(value)
}

/// Computes the total value locked in this contract
pub fn compute_total_value(deps: Deps, env: Env) -> StdResult<Uint128> {
    // Get all assets from storage
    let mut all_assets = VAULT_ASSETS
        .range(deps.storage, None, None, Order::Ascending)
        .collect::<StdResult<Vec<(AssetEntry, ProxyAsset)>>>()?;

    let mut total_value = Uint128::zero();
    let memory = MEMORY.load(deps.storage)?;
    // Calculate their value iteratively
    for vault_asset_entry in all_assets.iter_mut() {
        total_value += vault_asset_entry.1.value(deps, &env, &memory, None)?;
    }
    Ok(total_value)
}

pub fn query_proxy_asset_validity(deps: Deps) -> StdResult<ValidityResponse> {
    // assets that resolve and have valid value-references
    let mut checked_assets: HashSet<String> = HashSet::new();
    // assets that don't resolve, they have a missing dependency
    let mut unresolvable_assets: HashSet<String> = HashSet::new();
    // assets that are missing
    let mut missing_assets: HashSet<String> = HashSet::new();
    let mut base_asset: Option<String> = None;

    let assets = VAULT_ASSETS
        .range(deps.storage, None, None, Order::Ascending)
        .collect::<StdResult<Vec<(AssetEntry, ProxyAsset)>>>()?;
    for (_, asset) in assets {
        resolve_asset(
            deps,
            &mut checked_assets,
            &mut unresolvable_assets,
            &mut missing_assets,
            asset,
            &mut base_asset,
        )?;
    }

    let unresolvable_assets_resp = {
        if unresolvable_assets.is_empty() {
            None
        } else {
            Some(
                unresolvable_assets
                    .into_iter()
                    .map(|asset| asset.into())
                    .collect(),
            )
        }
    };

    let missing_assets_resp = {
        if missing_assets.is_empty() {
            None
        } else {
            Some(
                missing_assets
                    .into_iter()
                    .map(|asset| asset.into())
                    .collect(),
            )
        }
    };

    Ok(ValidityResponse {
        unresolvable_assets: unresolvable_assets_resp,
        missing_dependencies: missing_assets_resp,
    })
}

pub fn resolve_asset(
    deps: Deps,
    checked_assets: &mut HashSet<String>,
    unresolvable_assets: &mut HashSet<String>,
    missing_assets: &mut HashSet<String>,
    proxy_asset: ProxyAsset,
    base: &mut Option<String>,
) -> StdResult<()> {
    let ProxyAsset {
        asset: entry,
        value_reference,
    } = proxy_asset;
    // key already checked?
    if checked_assets.contains(entry.as_str()) || unresolvable_assets.contains(entry.as_str()) {
        return Ok(());
    }

    match value_reference {
        None => {
            if base.is_some() {
                if entry.as_str() != base.as_ref().unwrap() {
                    return Err(StdError::generic_err(format!(
                        "All assets accept the base asset must have a value reference. One of these assets is missing it: {}, {}",
                        base.as_ref().unwrap(),
                        entry.as_str()
                    )));
                }
            } else {
                *base = Some(entry.to_string());
            }
        }
        Some(value_ref) => {
            let asset_dependencies = get_value_ref_dependencies(&value_ref, entry.to_string());
            let mut loaded_dependencies = vec![];
            for asset in asset_dependencies {
                match try_load_asset(deps, missing_assets, asset) {
                    Some(proxy_asset) => {
                        // successfully loaded dependency
                        loaded_dependencies.push(proxy_asset)
                    }
                    None => {
                        // current asset unresolvable because it has dependencies that can't be loaded.
                        unresolvable_assets.insert(entry.to_string());
                    }
                }
            }
            // proceed with dependencies that resolved and add entry as checked
            checked_assets.insert(entry.to_string());
            for dep in loaded_dependencies {
                resolve_asset(
                    deps,
                    checked_assets,
                    unresolvable_assets,
                    missing_assets,
                    dep,
                    base,
                )?
            }
        }
    }
    Ok(())
}

pub fn try_load_asset(
    deps: Deps,
    missing_assets: &mut HashSet<String>,
    key: AssetEntry,
) -> Option<ProxyAsset> {
    let maybe_proxy_asset = VAULT_ASSETS.load(deps.storage, key.clone());
    match maybe_proxy_asset {
        Ok(asset) => Some(asset),
        Err(_) => {
            missing_assets.insert(key.to_string());
            None
        }
    }
}

pub fn get_value_ref_dependencies(value_reference: &ValueRef, entry: String) -> Vec<AssetEntry> {
    match value_reference {
        crate::proxy_asset::ValueRef::Pool { pair } => {
            // Check if the other asset in the pool resolves
            let other_pool_asset: AssetEntry = other_asset_name(entry.as_str(), &pair.contract)
                .unwrap()
                .into();
            vec![other_pool_asset]
        }
        crate::proxy_asset::ValueRef::LiquidityToken {} => {
            // check if both tokens of pool resolve
            let maybe_pair: UncheckedContractEntry = entry.try_into().unwrap();
            let other_pool_asset_names = get_pair_asset_names(maybe_pair.contract.as_str());
            let asset1: AssetEntry = other_pool_asset_names[0].into();
            let asset2: AssetEntry = other_pool_asset_names[1].into();
            vec![asset1, asset2]
        }
        crate::proxy_asset::ValueRef::ValueAs {
            asset,
            multiplier: _,
        } => vec![asset.clone()],
        crate::proxy_asset::ValueRef::External { api_name: _ } => todo!(),
    }
}

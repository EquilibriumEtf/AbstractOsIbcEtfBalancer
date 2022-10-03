use abstract_os::objects::{AssetEntry, ContractEntry};
use abstract_sdk::Resolve;
use client_osmo_bindings::OsmosisQuery;
use cosmwasm_std::{CosmosMsg, Decimal, DepsMut, Env, MessageInfo, Order, Response, StdError};

use crate::contract::{execute_ibc_query, ProxyResult};
use crate::error::ProxyError;
use crate::proxy_asset::{UncheckedProxyAsset, ValueRef};
use crate::queries::*;
use crate::state::{OraclePrice, POOL_PRICES, TWAP_STATE};
use crate::state::{ADMIN, MEMORY, STATE, VAULT_ASSETS};

const LIST_SIZE_LIMIT: usize = 15;
// 30 minute TWAP
const TWAP_INTERVAL: u64 = 60 * 30;

pub const TWAP_QUERY: &str = "twap";

pub fn update_prices(deps: DepsMut, info: MessageInfo, env: Env) -> ProxyResult {
    let keys_res: Result<Vec<ContractEntry>, _> = POOL_PRICES
        .keys(deps.storage, None, None, Order::Descending)
        .collect();
    let memory = MEMORY.load(deps.storage)?;
    let current_time = env.block.time.seconds();
    let pools = memory.query_contracts(deps.as_ref(), keys_res?)?;
    let mut queries = vec![];
    for (pool_name, pool_id) in pools {
        let ContractEntry {
            protocol: _,
            contract,
        } = pool_name;
        let lowercase = contract.to_ascii_lowercase();
        let mut composite: Vec<&str> = lowercase.split('_').collect();
        if composite.len() != 2 {
            return Err(ProxyError::Std(StdError::generic_err(
                "trading pair should be formatted as \"asset1_asset2\".",
            )));
        }
        composite.sort();
        let quote =
            AssetEntry::new(&format! {"osmo>{}",composite[0]}).resolve(deps.as_ref(), &memory)?;
        let quote = match quote {
            cw_asset::AssetInfoBase::Native(denom) => denom,
            _ => todo!(),
        };
        let base =
            AssetEntry::new(&format! {"osmo>{}",composite[1]}).resolve(deps.as_ref(), &memory)?;
        let base = match base {
            cw_asset::AssetInfoBase::Native(denom) => denom,
            _ => todo!(),
        };
        queries.push(cosmwasm_std::QueryRequest::Custom(
            OsmosisQuery::ArithmeticTwapToNow {
                id: pool_id.to_string().parse().unwrap(),
                quote_asset_denom: quote,
                base_asset_denom: base,
                start_time: (current_time - TWAP_INTERVAL) as i64,
            },
        ))
    }
    let twap_channel = TWAP_STATE.load(deps.storage)?;
    execute_ibc_query(
        deps,
        env,
        info,
        twap_channel.channel_id,
        queries,
        Some(TWAP_QUERY.into()),
    )
    .map_err(Into::into)
}

/// Executes actions forwarded by whitelisted contracts
/// This contracts acts as a proxy contract for the dApps
pub fn execute_action(deps: DepsMut, msg_info: MessageInfo, msgs: Vec<CosmosMsg>) -> ProxyResult {
    let state = STATE.load(deps.storage)?;
    if !state
        .modules
        .contains(&deps.api.addr_validate(msg_info.sender.as_str())?)
    {
        return Err(ProxyError::SenderNotWhitelisted {});
    }

    Ok(Response::new().add_messages(msgs))
}

/// Update the stored vault asset information
pub fn update_assets(
    deps: DepsMut,
    msg_info: MessageInfo,
    to_add: Vec<UncheckedProxyAsset>,
    to_remove: Vec<String>,
) -> ProxyResult {
    // Only Admin can call this method
    ADMIN.assert_admin(deps.as_ref(), &msg_info.sender)?;
    let memory = &MEMORY.load(deps.storage)?;
    // Check the vault size to be within the size limit to prevent running out of gas when doing lookups
    let current_vault_size = VAULT_ASSETS
        .keys(deps.storage, None, None, Order::Ascending)
        .count();
    let delta: i128 = to_add.len() as i128 - to_remove.len() as i128;
    if current_vault_size as i128 + delta > LIST_SIZE_LIMIT as i128 {
        return Err(ProxyError::AssetsLimitReached {});
    }

    for new_asset in to_add.into_iter() {
        let checked_asset = new_asset.check(deps.as_ref(), memory)?;
        match &checked_asset.value_reference {
            Some(val_ref) => match val_ref {
                ValueRef::Pool { pair } => {
                    POOL_PRICES.save(
                        deps.storage,
                        pair.clone(),
                        &OraclePrice {
                            price: Decimal::zero(),
                        },
                    )?;
                }
                _ => panic!("not supported"),
            },
            _ => (),
        }
        VAULT_ASSETS.save(deps.storage, checked_asset.asset.clone(), &checked_asset)?;
    }

    for asset_id in to_remove {
        VAULT_ASSETS.remove(deps.storage, asset_id.into());
    }

    // Check validity of new configuration
    let validity_result = query_proxy_asset_validity(deps.as_ref())?;
    if validity_result.missing_dependencies.is_some()
        || validity_result.unresolvable_assets.is_some()
    {
        return Err(ProxyError::BadUpdate(format!("{:?}", validity_result)));
    }

    Ok(Response::new().add_attribute("action", "update_proxy_assets"))
}

/// Add a contract to the whitelist
pub fn add_module(deps: DepsMut, msg_info: MessageInfo, module: String) -> ProxyResult {
    ADMIN.assert_admin(deps.as_ref(), &msg_info.sender)?;

    let mut state = STATE.load(deps.storage)?;
    if state.modules.contains(&deps.api.addr_validate(&module)?) {
        return Err(ProxyError::AlreadyInList {});
    }

    // This is a limit to prevent potentially running out of gas when doing lookups on the modules list
    if state.modules.len() >= LIST_SIZE_LIMIT {
        return Err(ProxyError::ModuleLimitReached {});
    }

    // Add contract to whitelist.
    state.modules.push(deps.api.addr_validate(&module)?);
    STATE.save(deps.storage, &state)?;

    // Respond and note the change
    Ok(Response::new().add_attribute("Added contract to whitelist: ", module))
}

/// Remove a contract from the whitelist
pub fn remove_module(deps: DepsMut, msg_info: MessageInfo, module: String) -> ProxyResult {
    ADMIN.assert_admin(deps.as_ref(), &msg_info.sender)?;

    let mut state = STATE.load(deps.storage)?;
    if !state.modules.contains(&deps.api.addr_validate(&module)?) {
        return Err(ProxyError::NotInList {});
    }

    // Remove contract from whitelist.
    let module_address = deps.api.addr_validate(&module)?;
    state.modules.retain(|addr| *addr != module_address);
    STATE.save(deps.storage, &state)?;

    // Respond and note the change
    Ok(Response::new().add_attribute("Removed contract from whitelist: ", module))
}

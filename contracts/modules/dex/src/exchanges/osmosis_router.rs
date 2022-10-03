use crate::{
    contract::{DexApi, DexResult},
    error::DexError,
    DEX,
};

use abstract_os::{
    manager::state::OS_MODULES,
    objects::{AssetEntry, ContractEntry},
    PROXY,
};
use abstract_sdk::{MemoryOperation, OsExecute, Resolve};
use client_osmo_bindings::{OsmosisMsg, SwapAmountWithLimit};
use cosmwasm_std::{wasm_execute, Coin, CosmosMsg, Decimal, Response, Uint128, Addr};
use cw_asset::{Asset, AssetInfo};
use simple_ica_client::msg::ExecuteMsg as ProxyExecute;
// use simple_ica::osmosis_router_msg::ExecuteMsg;

pub const OSMOSISROUTER: &str = "osmosisrouter";
pub struct OsmosisRouter {}

impl DEX for OsmosisRouter {
    fn name(&self) -> &'static str {
        OSMOSISROUTER
    }

    fn swap(
        &self,
        deps: cosmwasm_std::Deps,
        api: DexApi,
        pair_address: Addr,
        offer_asset: cw_asset::Asset,
        ask_asset: cw_asset::AssetInfo,
        belief_price: Option<cosmwasm_std::Decimal>,
        max_spread: Option<cosmwasm_std::Decimal>,
    ) -> DexResult {
        let input_coin = match &offer_asset.info {
            cw_asset::AssetInfoBase::Native(denom) => {
                Ok(Coin::new(offer_asset.amount.u128(), denom))
            }
            _ => Err(DexError::Cw1155Unsupported),
        }?;

        let memory = api.load_memory(deps.storage)?;

        let (denom_in, denom_out) = if input_coin.denom == "ujunox" {
            (
                memory.query_asset(deps, &AssetEntry::new(&format!("osmo>junox")))?,
                memory.query_asset(deps, &AssetEntry::new(&format!("osmo>osmo")))?,
            )
        } else {
            (
                memory.query_asset(deps, &AssetEntry::new(&format!("osmo>osmo")))?,
                memory.query_asset(deps, &AssetEntry::new(&format!("osmo>junox")))?,
            )
        };

        let output_denom = match &denom_out {
            cw_asset::AssetInfoBase::Native(denom) => Ok(denom.to_string()),
            _ => Err(DexError::Cw1155Unsupported),
        }?;

        let input_denom = match &denom_in {
            cw_asset::AssetInfoBase::Native(denom) => Ok(denom.to_string()),
            _ => Err(DexError::Cw1155Unsupported),
        }?;

        let swap_msg = CosmosMsg::Custom(OsmosisMsg::simple_swap(
            pair_address.to_string().parse().unwrap(),
            input_denom,
            output_denom,
            SwapAmountWithLimit::ExactIn {
                input: input_coin.amount,
                min_output: cosmwasm_std::Uint128::zero(),
            },
        ));

        // swap msg
        let proxy_msg = ProxyExecute::SendMsgs {
            msgs: vec![swap_msg],
        };
        let proxy_addr = api.target()?;
        let swap_msg = wasm_execute(proxy_addr.clone(), &proxy_msg, vec![])?;

        // send over transfer channel
        let transfer_channel_id = ContractEntry {
            protocol: "hermes".to_string(),
            contract: "juno>osmo".to_string(),
        }
        .resolve(deps, &memory)?
        .to_string();
        // send msg
        let proxy_msg = ProxyExecute::SendFunds {
            transfer_channel_id,
            coins: vec![input_coin],
        };
        let send_msg = wasm_execute(proxy_addr, &proxy_msg, vec![])?;

        Ok(Response::new().add_message(send_msg).add_message(swap_msg))
    }

    fn provide_liquidity(
        &self,
        deps: cosmwasm_std::Deps,
        api: DexApi,
        pair_address: cosmwasm_std::Addr,
        offer_assets: Vec<cw_asset::Asset>,
        max_spread: Option<cosmwasm_std::Decimal>,
    ) -> DexResult {
        todo!()
    }

    fn provide_liquidity_symmetric(
        &self,
        deps: cosmwasm_std::Deps,
        api: DexApi,
        pair_address: cosmwasm_std::Addr,
        offer_asset: cw_asset::Asset,
        paired_assets: Vec<cw_asset::AssetInfo>,
    ) -> DexResult {
        todo!()
    }

    fn withdraw_liquidity(
        &self,
        deps: cosmwasm_std::Deps,
        api: &DexApi,
        pair_address: cosmwasm_std::Addr,
        lp_token: cw_asset::Asset,
    ) -> DexResult {
        todo!()
    }

    fn simulate_swap(
        &self,
        deps: cosmwasm_std::Deps,
        pair_address: cosmwasm_std::Addr,
        offer_asset: cw_asset::Asset,
        ask_asset: cw_asset::AssetInfo,
    ) -> std::result::Result<(Uint128, Uint128, Uint128, bool), DexError> {
        todo!()
    }
}

fn coins_in_assets(assets: &[Asset]) -> Vec<Coin> {
    let mut coins = vec![];
    for asset in assets {
        if let AssetInfo::Native(denom) = &asset.info {
            coins.push(Coin::new(asset.amount.u128(), denom.clone()));
        }
    }
    coins
}

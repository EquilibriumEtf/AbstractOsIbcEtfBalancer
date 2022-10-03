//! # Liquidity Interface Add-On
//!
//! `abstract_os::etf` is an add-on which allows users to deposit into or withdraw from a [`crate::proxy`] contract.
//!
//! ## Description
//! This contract uses the proxy's value calculation configuration to get the value of the assets held in the proxy and the relative value of the deposit asset.
//! It then mints LP tokens that are claimable for an equal portion of the proxy assets at a later date.
//!
//! ---
//! **WARNING:** This mint/burn mechanism can be mis-used by flash-loan attacks if the assets contained are of low-liquidity compared to the etf's size.
//!
//! ## Creation
//! The etf contract can be added on an OS by calling [`ExecuteMsg::CreateModule`](crate::manager::ExecuteMsg::CreateModule) on the manager of the os.
//! ```ignore
//! let etf_init_msg = InstantiateMsg{
//!                deposit_asset: "juno".to_string(),
//!                base: BaseInstantiateMsg{memory_address: "juno1...".to_string()},
//!                fee: Decimal::percent(10),
//!                provider_addr: "juno1...".to_string(),
//!                token_code_id: 3,
//!                etf_lp_token_name: Some("demo_etf".to_string()),
//!                etf_lp_token_symbol: Some("DEMO".to_string()),
//!        };
//! let create_module_msg = ExecuteMsg::CreateModule {
//!                 module: Module {
//!                     info: ModuleInfo {
//!                         name: ETF.into(),
//!                         version: None,
//!                     },
//!                     kind: crate::core::modules::ModuleKind::External,
//!                 },
//!                 init_msg: Some(to_binary(&etf_init_msg).unwrap()),
//!        };
//! // Call create_module_msg on manager
//! ```
//!
//! ## Migration
//! Migrating this contract is done by calling `ExecuteMsg::Upgrade` on [`crate::manager`] with `crate::ETF` as module.

#[cosmwasm_schema::cw_serde]
pub struct WeightedAsset {
    pub weight: u64,
    /// asset name
    pub identifier: AssetEntry,
}

pub mod state {
    use schemars::JsonSchema;
    use serde::{Deserialize, Serialize};

    use cosmwasm_std::Decimal;
    use cw_storage_plus::Item;

    use super::WeightedAsset;

    #[derive(Serialize, Deserialize, Clone, Debug, Eq, PartialEq, JsonSchema)]
    /// State stores LP token address
    /// BaseState is initialized in contract
    pub struct State {
        // the allowed deviation from the target ratio
        pub max_deviation: Decimal,
        // the dex to use for swaps
        pub dex: String,
    }

    pub const STATE: Item<State> = Item::new("\u{0}{5}state");
    pub const ASSET_WEIGHTS: Item<Vec<WeightedAsset>> = Item::new("\u{0}{6}assets");
}

use cosmwasm_std::Decimal;

use crate::{
    add_on::{BaseExecuteMsg, BaseInstantiateMsg, BaseQueryMsg},
    objects::AssetEntry,
};

/// Migrate msg
#[cosmwasm_schema::cw_serde]
pub struct MigrateMsg {}

/// Init msg
#[cosmwasm_schema::cw_serde]
pub struct InstantiateMsg {
    /// Base init msg, sets memory address
    pub base: BaseInstantiateMsg,
    /// Weights of the assets in the etf
    pub asset_weights: Vec<WeightedAsset>,
    /// The allowed deviation from the target ratio
    pub deviation: Decimal,
    /// The dex to use for swaps
    pub dex: String,
}

#[cosmwasm_schema::cw_serde]
pub enum ExecuteMsg {
    /// Execute on the base-add-on contract logic
    Base(BaseExecuteMsg),
    /// Rebalance the etf
    Rebalance {},
    /// Update asset weights
    UpdateAssetWeights {
        to_add: Option<Vec<WeightedAsset>>,
        to_remove: Option<Vec<String>>,
    },
    /// Update config
    UpdateConfig {
        deviation: Option<Decimal>,
        dex: Option<String>,
    },
}

#[cosmwasm_schema::cw_serde]
pub enum QueryMsg {
    Base(BaseQueryMsg),
    // Add dapp-specific queries here
    /// Returns [`StateResponse`]
    State {},
    // /// Returns [`AssetWeightsResponse`]
    // /// Returns the actual weights of the assets in the etf
    // AssetWeights {},
}

#[cosmwasm_schema::cw_serde]
pub enum DepositHookMsg {
    WithdrawLiquidity {},
    ProvideLiquidity {},
}

#[cosmwasm_schema::cw_serde]
pub struct StateResponse {
    pub asset_weights: Vec<WeightedAsset>,
    pub max_deviation: Decimal,
}

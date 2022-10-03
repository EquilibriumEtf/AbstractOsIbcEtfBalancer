//! # Decentralized Exchange API
//!
//! `abstract_os::dex` is a generic dex-interfacing contract that handles address retrievals and dex-interactions.

use cosmwasm_schema::QueryResponses;
use cosmwasm_std::{Decimal, Uint128};

use crate::objects::{AssetEntry, ContractEntry};

type DexName = String;
pub type OfferAsset = (AssetEntry, Uint128);

/// Dex Execute msg
#[cosmwasm_schema::cw_serde]
pub enum RequestMsg {
    ProvideLiquidity {
        // support complex pool types
        /// Assets to add
        assets: Vec<OfferAsset>,
        /// Name of the Dex to use.
        dex: Option<DexName>,
        max_spread: Option<Decimal>,
    },
    ProvideLiquiditySymmetric {
        offer_asset: OfferAsset,
        // support complex pool types
        /// Assets that are paired with the offered asset
        paired_assets: Vec<AssetEntry>,
        /// Name of the Dex to use.
        dex: Option<DexName>,
    },
    WithdrawLiquidity {
        lp_token: AssetEntry,
        amount: Uint128,
        dex: Option<DexName>,
    },
    Swap {
        offer_asset: OfferAsset,
        ask_asset: AssetEntry,
        dex: Option<DexName>,
        max_spread: Option<Decimal>,
        belief_price: Option<Decimal>,
    },
}

#[cosmwasm_schema::cw_serde]
#[derive(QueryResponses)]
pub enum ApiQueryMsg {
    #[returns(SimulateSwapResponse)]
    SimulateSwap {
        offer_asset: OfferAsset,
        ask_asset: AssetEntry,
        dex: Option<DexName>,
    },
}

// LP/protocol fees could be withheld from either input or output so commission asset must be included.
#[cosmwasm_schema::cw_serde]
pub struct SimulateSwapResponse {
    pub pool: ContractEntry,
    /// Amount you would receive when performing the swap.
    pub return_amount: Uint128,
    /// Spread in ask_asset for this swap
    pub spread_amount: Uint128,
    /// Commission charged for the swap
    pub commission: (AssetEntry, Uint128),
}

use cosmwasm_std::{Coin, CosmosMsg, QueryRequest, Timestamp};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use simple_ica::{ReceiveIcaResponseMsg, StdAck};

use crate::{
    proxy_asset::{ProxyAsset, UncheckedProxyAsset},
    state::AccountData,
};
use client_osmo_bindings::{OsmosisMsg, OsmosisQuery};
/// This needs no info. Owner of the contract is whoever signed the InstantiateMsg.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub struct InstantiateMsg {
    pub os_id: u32,
    pub memory_address: String,
}

use cosmwasm_std::{Empty, Uint128};

use abstract_os::objects::AssetEntry;

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ExecuteMsg {
    SetTWAPChannel(String),
    UpdatePrices {},
    /// Sets the admin
    SetAdmin {
        admin: String,
    },
    /// Executes the provided messages if sender is whitelisted
    ModuleAction {
        msgs: Vec<CosmosMsg<Empty>>,
    },
    /// Adds the provided address to whitelisted dapps
    AddModule {
        module: String,
    },
    /// Removes the provided address from the whitelisted dapps
    RemoveModule {
        module: String,
    },
    /// Updates the VAULT_ASSETS map
    UpdateAssets {
        to_add: Vec<UncheckedProxyAsset>,
        to_remove: Vec<String>,
    },
    SendMsgs {
        /// Note: we don't handle custom messages on remote chains
        msgs: Vec<CosmosMsg<OsmosisMsg>>,
    },
    CheckRemoteBalance {
        channel_id: String,
    },
    IbcQuery {
        channel_id: String,
        msgs: Vec<QueryRequest<OsmosisQuery>>,
        /// If set, the original caller will get a callback with of the result, along with this id
        callback_id: Option<String>,
    },
    /// If you sent funds to this contract, it will attempt to ibc transfer them
    /// to the account on the remote side of this channel.
    /// If we don't have the address yet, this fails.
    SendFunds {
        /// The channel to use for ibctransfer. This is bound to a different
        /// port and handled by a different module.
        /// It should connect to the same chain as the ica_channel_id does
        transfer_channel_id: String,
        coins: Vec<Coin>,
    },
    ReceiveIcaResponse(ReceiveIcaResponseMsg),
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct MigrateMsg {}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum QueryMsg {
    /// Returns [`ConfigResponse`]
    Config {},
    /// Returns the total value of all held assets
    /// [`TotalValueResponse`]
    TotalValue {},
    /// Returns the value of one specific asset
    /// [`HoldingValueResponse`]
    HoldingValue {
        identifier: String,
    },
    /// Returns the amount of specified tokens this contract holds
    /// [`HoldingAmountResponse`]
    HoldingAmount {
        identifier: String,
    },
    /// Returns the VAULT_ASSETS value for the specified key
    /// [`AssetConfigResponse`]
    AssetConfig {
        identifier: String,
    },
    /// Returns [`AssetsResponse`]
    Assets {
        page_token: Option<String>,
        page_size: Option<u8>,
    },
    /// Returns [`ValidityResponse`]
    CheckValidity {},
    /// Returns [`BaseAssetResponse`]
    BaseAsset {},
    // Returns current admin
    Admin {},
    // Shows all open accounts (incl. remote info)
    ListAccounts {},
    // Get account for one channel
    Account {
        channel_id: String,
    },
    // Get latest query
    LatestQueryResult {
        channel_id: String,
    },
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct AdminResponse {
    pub admin: String,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct ListAccountsResponse {
    pub accounts: Vec<AccountInfo>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct LatestQueryResponse {
    /// last block balance was updated (0 is never)
    pub last_update_time: Timestamp,
    pub response: StdAck,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct AccountInfo {
    pub channel_id: String,
    /// last block balance was updated (0 is never)
    pub last_update_time: Timestamp,
    /// in normal cases, it should be set, but there is a delay between binding
    /// the channel and making a query and in that time it is empty
    pub remote_addr: Option<String>,
    pub remote_balance: Vec<Coin>,
}

impl AccountInfo {
    pub fn convert(channel_id: String, input: AccountData) -> Self {
        AccountInfo {
            channel_id,
            last_update_time: input.last_update_time,
            remote_addr: input.remote_addr,
            remote_balance: input.remote_balance,
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct AccountResponse {
    /// last block balance was updated (0 is never)
    pub last_update_time: Timestamp,
    /// in normal cases, it should be set, but there is a delay between binding
    /// the channel and making a query and in that time it is empty
    pub remote_addr: Option<String>,
    pub remote_balance: Vec<Coin>,
}

impl From<AccountData> for AccountResponse {
    fn from(input: AccountData) -> Self {
        AccountResponse {
            last_update_time: input.last_update_time,
            remote_addr: input.remote_addr,
            remote_balance: input.remote_balance,
        }
    }
}

#[cosmwasm_schema::cw_serde]
pub struct ConfigResponse {
    pub modules: Vec<String>,
}

#[cosmwasm_schema::cw_serde]
pub struct TotalValueResponse {
    pub value: Uint128,
}

#[cosmwasm_schema::cw_serde]
pub struct HoldingValueResponse {
    pub value: Uint128,
}

#[cosmwasm_schema::cw_serde]
pub struct ValidityResponse {
    /// Assets that have unresolvable dependencies in their value calculation
    pub unresolvable_assets: Option<Vec<AssetEntry>>,
    /// Assets that are missing in the VAULT_ASSET map which caused some assets to be unresolvable.
    pub missing_dependencies: Option<Vec<AssetEntry>>,
}

#[cosmwasm_schema::cw_serde]
pub struct BaseAssetResponse {
    pub base_asset: ProxyAsset,
}

#[cosmwasm_schema::cw_serde]
pub struct HoldingAmountResponse {
    pub amount: Uint128,
}

#[cosmwasm_schema::cw_serde]
pub struct AssetConfigResponse {
    pub proxy_asset: ProxyAsset,
}

#[cosmwasm_schema::cw_serde]
pub struct AssetsResponse {
    pub assets: Vec<(AssetEntry, ProxyAsset)>,
}

/// Query message to external contract to get asset value
#[cosmwasm_schema::cw_serde]

pub struct ValueQueryMsg {
    pub asset: AssetEntry,
    pub amount: Uint128,
}
/// External contract value response
#[cosmwasm_schema::cw_serde]
pub struct ExternalValueResponse {
    pub value: Uint128,
}

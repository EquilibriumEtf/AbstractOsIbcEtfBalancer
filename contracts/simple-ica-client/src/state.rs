use abstract_os::objects::ContractEntry;
use serde::{Deserialize, Serialize};

use crate::msg::LatestQueryResponse;
use cosmwasm_std::{Addr, Coin, Decimal, Timestamp};
use cw_storage_plus::{Item, Map};

#[derive(Serialize, Deserialize, Clone, Default, Debug, PartialEq)]
pub struct AccountData {
    /// last block balance was updated (0 is never)
    pub last_update_time: Timestamp,
    /// In normal cases, it should be set, but there is a delay between binding
    /// the channel and making a query and in that time it is empty.
    ///
    /// Since we do not have a way to validate the remote address format, this
    /// must not be of type `Addr`.
    pub remote_addr: Option<String>,
    pub remote_balance: Vec<Coin>,
}

#[cosmwasm_schema::cw_serde]
pub struct OraclePrice {
    pub price: Decimal,
}

#[cosmwasm_schema::cw_serde]
pub struct TWAPInfo {
    pub channel_id: String,
    pub last_update: u64,
}

pub const POOL_PRICES: Map<ContractEntry, OraclePrice> = Map::new("pools");
pub const ACCOUNTS: Map<&str, AccountData> = Map::new("accounts");
pub const LATEST_QUERIES: Map<&str, LatestQueryResponse> = Map::new("querys");
pub const TWAP_STATE: Item<TWAPInfo> = Item::new("twap_channel");
pub const RETRIES: Item<u8> = Item::new("test");

use crate::proxy_asset::ProxyAsset;

pub use abstract_os::objects::core::OS_ID;
use cw_controllers::Admin;

use abstract_os::objects::{memory::Memory, AssetEntry};
#[cosmwasm_schema::cw_serde]
pub struct State {
    pub modules: Vec<Addr>,
}

pub const MEMORY: Item<Memory> = Item::new("\u{0}{6}memory");
pub const STATE: Item<State> = Item::new("\u{0}{5}state");
pub const ADMIN: Admin = Admin::new("admin");
pub const VAULT_ASSETS: Map<AssetEntry, ProxyAsset> = Map::new("proxy_assets");

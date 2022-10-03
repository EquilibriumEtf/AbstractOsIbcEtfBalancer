use cosmwasm_std::{CosmosMsg, QueryRequest};
use schemars::JsonSchema;

use client_osmo_bindings::{OsmosisMsg, OsmosisQuery};
use serde::{Deserialize, Serialize};

/// This is the message we send over the IBC channel
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum PacketMsg {
    Dispatch {
        sender: String,
        msgs: Vec<CosmosMsg<OsmosisMsg>>,
        callback_id: Option<String>,
    },
    IbcQuery {
        sender: String,
        msgs: Vec<QueryRequest<OsmosisQuery>>,
        callback_id: Option<String>,
    },
    WhoAmI {},
    Balances {},
    SendAllBack {
        sender: String,
        transfer_channel: String,
    },
}

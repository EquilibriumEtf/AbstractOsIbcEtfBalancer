use std::str::FromStr;

use crate::msg::{
    AssetConfigResponse, BaseAssetResponse, HoldingAmountResponse, HoldingValueResponse,
};
use crate::proxy_asset::ProxyAsset;
use crate::state::{State, TWAPInfo, ADMIN, MEMORY, OS_ID, STATE, VAULT_ASSETS, RETRIES};
use abstract_os::objects::{AssetEntry, ContractEntry};
use abstract_os::proxy::MigrateMsg;
use abstract_os::IBC_PROXY;
use abstract_sdk::memory::Memory;
use abstract_sdk::{Resolve, CONTRACT_VERSION};
#[cfg(not(feature = "library"))]
use cosmwasm_std::entry_point;
use cosmwasm_std::{
    from_binary, to_binary, Coin, CosmosMsg, Decimal, Deps, DepsMut, Empty, Env, IbcMsg,
    MessageInfo, Order, QueryRequest, QueryResponse, Response, StdError, StdResult,
};
use osmosis_std::types::osmosis::twap::v1beta1::ArithmeticTwapToNowResponse;

use crate::commands::*;
use crate::error::ProxyError;
use crate::ibc::PACKET_LIFETIME;
use crate::msg::TotalValueResponse;
use crate::msg::{
    AccountInfo, AccountResponse, AdminResponse, ExecuteMsg, InstantiateMsg, LatestQueryResponse,
    ListAccountsResponse, QueryMsg,
};
use crate::queries::*;
use crate::state::{OraclePrice, ACCOUNTS, LATEST_QUERIES, POOL_PRICES, TWAP_STATE};
use client_osmo_bindings::{OsmosisMsg, OsmosisQuery};
use cw2::set_contract_version;
use simple_ica::client_ibc_msg::PacketMsg;
use simple_ica::{IbcQueryResponse, ReceiveIcaResponseMsg, StdAck};
pub type ProxyResult = Result<Response, ProxyError>;

const OSMO_SWAP: &str = "twap";

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn instantiate(
    deps: DepsMut,
    _env: Env,
    info: MessageInfo,
    msg: InstantiateMsg,
) -> StdResult<Response> {
    // Use CW2 to set the contract version, this is needed for migrations
    set_contract_version(deps.storage, IBC_PROXY, CONTRACT_VERSION)?;
    OS_ID.save(deps.storage, &msg.os_id)?;
    STATE.save(deps.storage, &State { modules: vec![] })?;
    RETRIES.save(deps.storage, &0u8)?;
    TWAP_STATE.save(
        deps.storage,
        &TWAPInfo {
            channel_id: "".into(),
            last_update: 0u64,
        },
    )?;
    MEMORY.save(
        deps.storage,
        &Memory {
            address: deps.api.addr_validate(&msg.memory_address)?,
        },
    )?;
    let admin_addr = Some(info.sender);
    ADMIN.set(deps, admin_addr)?;

    Ok(Response::new().add_attribute("action", "instantiate"))
}
#[cfg_attr(not(feature = "library"), entry_point)]
pub fn migrate(deps: DepsMut, _env: Env, _msg: MigrateMsg) -> ProxyResult {
    RETRIES.save(deps.storage, &0u8)?;
    Ok(Response::default())
}
#[cfg_attr(not(feature = "library"), entry_point)]
pub fn execute(deps: DepsMut, env: Env, info: MessageInfo, msg: ExecuteMsg) -> ProxyResult {
    match msg {
        ExecuteMsg::SetTWAPChannel(channel_id) => {
            let last_update = TWAP_STATE.load(deps.storage)?.last_update;
            // req verification in future
            TWAP_STATE.save(
                deps.storage,
                &TWAPInfo {
                    channel_id,
                    last_update,
                },
            )?;
            Ok(Response::new())
        }
        ExecuteMsg::UpdatePrices {} => update_prices(deps, info, env),
        ExecuteMsg::ModuleAction { msgs } => execute_action(deps, info, msgs),
        ExecuteMsg::AddModule { module } => add_module(deps, info, module),
        ExecuteMsg::RemoveModule { module } => remove_module(deps, info, module),
        ExecuteMsg::UpdateAssets { to_add, to_remove } => {
            Ok(update_assets(deps, info, to_add, to_remove).unwrap())
        }
        ExecuteMsg::SetAdmin { admin } => {
            let admin_addr = deps.api.addr_validate(&admin)?;
            let previous_admin = ADMIN.get(deps.as_ref())?.unwrap();
            ADMIN
                .execute_update_admin::<Empty, Empty>(deps, info, Some(admin_addr))
                .unwrap();
            Ok(Response::default()
                .add_attribute("previous admin", previous_admin)
                .add_attribute("admin", admin))
        }
        ExecuteMsg::SendMsgs { msgs } => {
            execute_send_msgs(deps, env, info, msgs).map_err(Into::into)
        }
        ExecuteMsg::CheckRemoteBalance { channel_id } => {
            execute_check_remote_balance(deps, env, info, channel_id).map_err(Into::into)
        }
        ExecuteMsg::IbcQuery {
            channel_id,
            msgs,
            callback_id,
        } => execute_ibc_query(deps, env, info, channel_id, msgs, callback_id).map_err(Into::into),
        ExecuteMsg::SendFunds {
            transfer_channel_id,
            coins,
        } => execute_send_funds(deps, env, info, coins, transfer_channel_id).map_err(Into::into),
        ExecuteMsg::ReceiveIcaResponse(resp) => handle_ica_resp(deps, env, resp),
    }
}

pub fn handle_ica_resp(deps: DepsMut, env: Env, resp: ReceiveIcaResponseMsg) -> ProxyResult {
    let ReceiveIcaResponseMsg { id, msg } = resp;

    let unwrap_res = match msg {
        StdAck::Result(binary) => Ok(binary),
        StdAck::Error(err) => Err(ProxyError::Std(StdError::generic_err(err))),
    }?;

    if id == TWAP_QUERY {
        let keys_res: Result<Vec<ContractEntry>, _> = POOL_PRICES
            .keys(deps.storage, None, None, Order::Descending)
            .collect();
        let twap_results: IbcQueryResponse = from_binary(&unwrap_res)?;
        let decoded: StdResult<Vec<ArithmeticTwapToNowResponse>> = twap_results
            .results
            .iter()
            .map(|binar| from_binary(binar))
            .collect();
        let values: Vec<(ContractEntry, ArithmeticTwapToNowResponse)> =
            keys_res?.into_iter().zip(decoded?).collect();
        for (pool, price) in values {
            POOL_PRICES.save(
                deps.storage,
                pool,
                &OraclePrice {
                    price: Decimal::from_str(&price.arithmetic_twap)?,
                },
            )?;
        }
    } else if id == OSMO_SWAP {
        // Send everything back
        let twap_info = TWAP_STATE.load(deps.storage)?;
        let mem = MEMORY.load(deps.storage)?;
        // get channel id for osmo-> juno transfers
        let osmo_to_juno_channel = mem.query_contract(
            deps.as_ref(),
            &ContractEntry {
                protocol: "hermes".into(),
                contract: "osmo>juno".into(),
            },
        )?;
        let packet = PacketMsg::SendAllBack {
            sender: env.contract.address.to_string(),
            transfer_channel: osmo_to_juno_channel.to_string(),
        };
        let msg = IbcMsg::SendPacket {
            channel_id: twap_info.channel_id,
            data: to_binary(&packet)?,
            timeout: env.block.time.plus_seconds(PACKET_LIFETIME).into(),
        };
        return Ok(Response::new().add_message(msg));
    }
    Ok(Response::new())
}

pub fn execute_send_msgs(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    msgs: Vec<CosmosMsg<OsmosisMsg>>,
) -> StdResult<Response> {
    // auth check
    let state = STATE.load(deps.storage)?;
    if !state.modules.contains(&info.sender) {
        return Err(StdError::generic_err("Only admin may send messages"));
    }
    let twap = TWAP_STATE.load(deps.storage)?;
    let callback_id = Some(OSMO_SWAP.to_string());
    // ensure the channel exists (not found if not registered)
    ACCOUNTS.load(deps.storage, &twap.channel_id)?;

    // construct a packet to send
    let sender = env.contract.address.into();
    let packet = PacketMsg::Dispatch {
        sender,
        msgs,
        callback_id,
    };
    let msg = IbcMsg::SendPacket {
        channel_id: twap.channel_id,
        data: to_binary(&packet)?,
        timeout: env.block.time.plus_seconds(PACKET_LIFETIME).into(),
    };

    let res = Response::new()
        .add_message(msg)
        .add_attribute("action", "handle_send_msgs");
    Ok(res)
}

pub fn execute_ibc_query(
    _deps: DepsMut,
    env: Env,
    info: MessageInfo,
    channel_id: String,
    msgs: Vec<QueryRequest<OsmosisQuery>>,
    callback_id: Option<String>,
) -> StdResult<Response> {
    // construct a packet to send
    let sender = info.sender.into();
    let packet = PacketMsg::IbcQuery {
        sender,
        msgs,
        callback_id,
    };
    let msg = IbcMsg::SendPacket {
        channel_id,
        data: to_binary(&packet)?,
        timeout: env.block.time.plus_seconds(PACKET_LIFETIME).into(),
    };

    let res = Response::new()
        .add_message(msg)
        .add_attribute("action", "handle_check_remote_balance");
    Ok(res)
}

pub fn execute_check_remote_balance(
    deps: DepsMut,
    env: Env,
    _info: MessageInfo,
    channel_id: String,
) -> StdResult<Response> {
    // ensure the channel exists (not found if not registered)
    ACCOUNTS.load(deps.storage, &channel_id)?;

    // construct a packet to send
    let packet = PacketMsg::Balances {};
    let msg = IbcMsg::SendPacket {
        channel_id,
        data: to_binary(&packet)?,
        timeout: env.block.time.plus_seconds(PACKET_LIFETIME).into(),
    };

    let res = Response::new()
        .add_message(msg)
        .add_attribute("action", "handle_check_remote_balance");
    Ok(res)
}

pub fn execute_send_funds(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    coins: Vec<Coin>,
    transfer_channel_id: String,
) -> StdResult<Response> {
    // auth check
    let state = STATE.load(deps.storage)?;
    if !state.modules.contains(&info.sender) {
        return Err(StdError::generic_err("Only admin may send messages"));
    }

    let ica_channel_id = TWAP_STATE.load(deps.storage)?.channel_id;
    // load remote account
    let data = ACCOUNTS.load(deps.storage, &ica_channel_id)?;
    let remote_addr = match data.remote_addr {
        Some(addr) => addr,
        None => {
            return Err(StdError::generic_err(
                "We don't have the remote address for this channel",
            ))
        }
    };

    let mut msgs = vec![];
    for coin in coins {
        msgs.push(IbcMsg::Transfer {
            channel_id: transfer_channel_id.clone(),
            to_address: remote_addr.clone(),
            amount: coin,
            timeout: env.block.time.plus_seconds(PACKET_LIFETIME).into(),
        })
    }
    // construct a packet to send

    let res = Response::new()
        .add_messages(msgs)
        .add_attribute("action", "handle_send_funds");
    Ok(res)
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn query(deps: Deps, env: Env, msg: QueryMsg) -> StdResult<QueryResponse> {
    match msg {
        QueryMsg::Config {} => to_binary(&query_config(deps)?),
        QueryMsg::TotalValue {} => to_binary(&TotalValueResponse {
            value: compute_total_value(deps, env)?,
        }),
        QueryMsg::HoldingAmount { identifier } => {
            let vault_asset: AssetEntry = identifier.into();
            let memory = MEMORY.load(deps.storage)?;
            let asset_info = vault_asset.resolve(deps, &memory)?;
            to_binary(&HoldingAmountResponse {
                amount: asset_info.query_balance(&deps.querier, env.contract.address)?,
            })
        }
        QueryMsg::HoldingValue { identifier } => to_binary(&HoldingValueResponse {
            value: compute_holding_value(deps, &env, identifier)?,
        }),
        QueryMsg::AssetConfig { identifier } => to_binary(&AssetConfigResponse {
            proxy_asset: VAULT_ASSETS.load(deps.storage, identifier.into())?,
        }),
        QueryMsg::Assets {
            page_token,
            page_size,
        } => to_binary(&query_proxy_assets(deps, page_token, page_size)?),
        QueryMsg::CheckValidity {} => to_binary(&query_proxy_asset_validity(deps)?),
        QueryMsg::BaseAsset {} => {
            let res: Result<Vec<(AssetEntry, ProxyAsset)>, _> = VAULT_ASSETS
                .range(deps.storage, None, None, Order::Ascending)
                .collect();
            let maybe_base_asset: Vec<(AssetEntry, ProxyAsset)> = res?
                .into_iter()
                .filter(|(_, p)| p.value_reference.is_none())
                .collect();
            if maybe_base_asset.len() != 1 {
                Err(StdError::generic_err("No base asset configured."))
            } else {
                to_binary(&BaseAssetResponse {
                    base_asset: maybe_base_asset[0].1.to_owned(),
                })
            }
        }
        QueryMsg::Admin {} => to_binary(&query_admin(deps)?),
        QueryMsg::Account { channel_id } => to_binary(&query_account(deps, channel_id)?),
        QueryMsg::ListAccounts {} => to_binary(&query_list_accounts(deps)?),
        QueryMsg::LatestQueryResult { channel_id } => {
            to_binary(&query_latest_ibc_query_result(deps, channel_id)?)
        }
    }
}

fn query_account(deps: Deps, channel_id: String) -> StdResult<AccountResponse> {
    let account = ACCOUNTS.load(deps.storage, &channel_id)?;
    Ok(account.into())
}

fn query_latest_ibc_query_result(deps: Deps, channel_id: String) -> StdResult<LatestQueryResponse> {
    LATEST_QUERIES.load(deps.storage, &channel_id)
}

fn query_list_accounts(deps: Deps) -> StdResult<ListAccountsResponse> {
    let accounts = ACCOUNTS
        .range(deps.storage, None, None, Order::Ascending)
        .map(|r| {
            let (channel_id, account) = r?;
            Ok(AccountInfo::convert(channel_id, account))
        })
        .collect::<StdResult<_>>()?;
    Ok(ListAccountsResponse { accounts })
}

fn query_admin(deps: Deps) -> StdResult<AdminResponse> {
    let admin = ADMIN.get(deps)?.unwrap();
    Ok(AdminResponse {
        admin: admin.into(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use cosmwasm_std::testing::{mock_dependencies, mock_env, mock_info};

    const CREATOR: &str = "creator";

    #[test]
    fn instantiate_works() {
        let mut deps = mock_dependencies();
        let msg = InstantiateMsg {
            os_id: 1u32,
            memory_address: "testing_contract".to_string(),
        };
        let info = mock_info(CREATOR, &[]);
        let res = instantiate(deps.as_mut(), mock_env(), info, msg).unwrap();
        assert_eq!(0, res.messages.len());

        let admin = query_admin(deps.as_ref()).unwrap();
        assert_eq!(CREATOR, admin.admin.as_str());
    }
}

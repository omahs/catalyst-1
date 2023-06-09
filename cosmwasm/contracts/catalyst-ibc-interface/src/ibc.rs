#[cfg(not(feature = "library"))]
use cosmwasm_std::entry_point;
use cosmwasm_std::{
    DepsMut, Env, IbcChannelOpenMsg, IbcChannelConnectMsg, IbcBasicResponse, IbcChannelCloseMsg, 
    IbcPacketReceiveMsg, IbcReceiveResponse, IbcPacketAckMsg, IbcPacketTimeoutMsg, IbcChannel, IbcPacket, Binary, CosmosMsg, to_binary, SubMsg, Reply, Response, SubMsgResult
};

use catalyst_vault_common::msg::ExecuteMsg as SwapPoolExecuteMsg;

use crate::{ContractError, state::{IbcChannelInfo, OPEN_CHANNELS}, catalyst_ibc_payload::CatalystV1Packet, error::Never};


// NOTE: Large parts of this IBC section are based on the cw20-ics20 example repository.


// IBC Interface constants 
pub const CATALYST_V1_CHANNEL_VERSION: &str = "catalyst-v1";

pub const RECEIVE_REPLY_ID: u64 = 0x100;

pub const ACK_SUCCESS: u8 = 0;
pub const ACK_FAIL: u8 = 1;



// Channel management ***********************************************************************************************************

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn ibc_channel_open(
    _deps: DepsMut,
    _env: Env,
    msg: IbcChannelOpenMsg
) -> Result<(), ContractError> {

    // Enforce the desired IBC protocol configuration
    validate_ibc_channel_config(msg.channel(), msg.counterparty_version())
}


#[cfg_attr(not(feature = "library"), entry_point)]
pub fn ibc_channel_connect(
    deps: DepsMut,
    _env: Env,
    msg: IbcChannelConnectMsg,
) -> Result<IbcBasicResponse, ContractError> {

    // Enforce the desired IBC protocol configuration
    validate_ibc_channel_config(msg.channel(), msg.counterparty_version())?;

    // Save the channel info
    let ibc_channel: IbcChannel = msg.into();
    OPEN_CHANNELS.save(
        deps.storage,
        &ibc_channel.endpoint.channel_id.clone(),
        &IbcChannelInfo {
            endpoint: ibc_channel.endpoint,
            counterparty_endpoint: ibc_channel.counterparty_endpoint,
            connection_id: ibc_channel.connection_id,
        }
    )?;

    Ok(IbcBasicResponse::default())
}


#[cfg_attr(not(feature = "library"), entry_point)]
pub fn ibc_channel_close(
    deps: DepsMut,
    _env: Env,
    msg: IbcChannelCloseMsg,
) -> Result<IbcBasicResponse, ContractError> {

    // TODO overhaul the following
    // To recover from a lost channel, a new channel has to be established (permissionless) and the Catalyst pools
    // that relied on the closed channel have to be set up with new 'pool connections' employing the new channel.
    
    // Remove the channel info from the list of open channels
    let ibc_channel: IbcChannel = msg.into();
    OPEN_CHANNELS.remove(
        deps.storage,
        &ibc_channel.endpoint.channel_id.clone()
    );

    Ok(IbcBasicResponse::default())
}



fn validate_ibc_channel_config(
    channel: &IbcChannel,
    counterparty_version: Option<&str>,
) -> Result<(), ContractError> {

    // Check the channel version on the local side
    if channel.version != CATALYST_V1_CHANNEL_VERSION {
        return Err(
            ContractError::InvalidIbcChannelVersion { version: channel.version.clone() }
        );
    }

    // Check the channel version of the remote side. Note this value is only set in OpenTry and OpenAck,
    // and will occur in either 'ibc_channel_open' or 'ibc_channel_connect'. This check assumes that
    // at some point the 'counterparty_version' will be specified. (Code taken from cw20-ics20)
    // TODO do we want to add an extra check to make sure that the counterparty_version is always checked at some point?
    if let Some(version) = counterparty_version {
        if version != CATALYST_V1_CHANNEL_VERSION {
            return Err(
                ContractError::InvalidIbcChannelVersion { version: version.to_string() }
            );
        }
    }

    //TODO channel ordering type not enforced. Do we want to enforce an unordered channel (like cw20-ics20)

    Ok(())
}




// Channel communication ********************************************************************************************************

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn ibc_packet_receive(
    deps: DepsMut,
    _env: Env,
    msg: IbcPacketReceiveMsg,
) -> Result<IbcReceiveResponse, Never> {

    // Invoke the receive function (either 'ReceiveAsset' or 'ReceiveLiquidity') of the destination pool.
    // This function should never error, rather it should send a failure message within the returned ack.
    on_packet_receive(deps, msg.packet)
        .or_else(|_| {
            Ok(IbcReceiveResponse::new()            //TODO add attributes?
                .set_ack(ack_fail())
            )
        })

}


// If the swap pool invocation errors (i.e. the submessage created within 'on_packet_receive'), return a custom fail ack.
// NOTE: this 'reply' code is needed, as the Catalyst protocol is not compatible with the default 'failed-ack' that is 
// generated by CosmWasm. 
#[cfg_attr(not(feature = "library"), entry_point)]
pub fn reply(
    _deps: DepsMut,
    _env: Env,
    reply: Reply
) -> Result<Response, ContractError> {
    match reply.id {
        RECEIVE_REPLY_ID => match reply.result {
            SubMsgResult::Ok(_) => Ok(Response::new()),
            SubMsgResult::Err(_) => Ok(Response::new().set_data(ack_fail()))
        },
        _ => Err(ContractError::UnknownReplyId { id: reply.id }),
    }
}


#[cfg_attr(not(feature = "library"), entry_point)]
pub fn ibc_packet_ack(
    deps: DepsMut,
    _env: Env,
    msg: IbcPacketAckMsg,
) -> Result<IbcBasicResponse, Never> {

    //TODO only the first byte of the response is checked, the rest is ignored. Do we want this?
    let ack = msg.acknowledgement.data.0.get(0);
    match ack {
        Some(ack_id) => {
            match ack_id {
                &ACK_SUCCESS => on_packet_success(deps, msg.original_packet),
                &ACK_FAIL => on_packet_failure(deps, msg.original_packet),
                _ => Ok(IbcBasicResponse::new())    // If ack type is not recognized, just exit without error   //TODO do we want this?
            }
        },
        None => Ok(IbcBasicResponse::new())         // If ack type is not recognized, just exit without error   //TODO do we want this?
    }
}


#[cfg_attr(not(feature = "library"), entry_point)]
pub fn ibc_packet_timeout(
    deps: DepsMut,
    _env: Env,
    msg: IbcPacketTimeoutMsg,
) -> Result<IbcBasicResponse, Never> {
    on_packet_failure(deps, msg.packet)
}



pub fn ack_success() -> Binary {
    Into::<Binary>::into(vec![ACK_SUCCESS])
}

pub fn ack_fail() -> Binary {
    Into::<Binary>::into(vec![ACK_FAIL])
}



pub fn on_packet_receive(
    deps: DepsMut,
    packet: IbcPacket
) -> Result<IbcReceiveResponse, ContractError> {

    let catalyst_packet = CatalystV1Packet::try_decode(packet.data)?;

    // Match payload type and build up the execute message
    let receive_asset_execute_msg: cosmwasm_std::WasmMsg = match catalyst_packet {
        CatalystV1Packet::SendAsset(payload) => {

            // Build execute message
            let to_account = payload.to_account_validated(deps.as_ref())?.into_string();     // Validate to_account  //TODO do we need to validate this?
            let parsed_calldata = payload.variable_payload.parse_calldata(deps.as_ref())?;
            Ok::<cosmwasm_std::WasmMsg, ContractError>(cosmwasm_std::WasmMsg::Execute {
                contract_addr: payload.to_pool_validated(deps.as_ref())?.into_string(),         // Validate to_pool     //TODO do we need to validated this?
                msg: to_binary(&SwapPoolExecuteMsg::<()>::ReceiveAsset {
                    channel_id: packet.dest.channel_id,
                    from_pool: payload.from_pool.to_binary(),                                      // Do not validate from_pool as its format is unknown. It is only used for logging
                    to_asset_index: payload.variable_payload.to_asset_index,
                    to_account,
                    u: payload.u,
                    min_out: payload.variable_payload.min_out()?,                               // Convert min_out into Uint128
                    from_amount: payload.variable_payload.from_amount,
                    from_asset: payload.variable_payload.from_asset.to_binary(),
                    from_block_number_mod: payload.variable_payload.block_number,
                    calldata_target: parsed_calldata.clone().map(|data| data.target),
                    calldata: parsed_calldata.map(|data| data.bytes)
                })?,
                funds: vec![]
            })

        },
        CatalystV1Packet::SendLiquidity(payload) => {

            // Build execute message
            let to_account = payload.to_account_validated(deps.as_ref())?.into_string();     // Validate to_account  //TODO do we need to validate this?
            let parsed_calldata = payload.variable_payload.parse_calldata(deps.as_ref())?;
            Ok::<cosmwasm_std::WasmMsg, ContractError>(cosmwasm_std::WasmMsg::Execute {
                contract_addr: payload.to_pool_validated(deps.as_ref())?.into_string(),         // Validate to_pool     //TODO do we need to validate this?
                msg: to_binary(&SwapPoolExecuteMsg::<()>::ReceiveLiquidity {
                    channel_id: packet.dest.channel_id,
                    from_pool: payload.from_pool.to_binary(),                                      // Do not validate from_pool as its format is unknown. It is only used for logging
                    to_account,
                    u: payload.u,
                    min_pool_tokens: payload.variable_payload.min_pool_tokens()?,                           // Convert min_pool_tokens into Uint128
                    min_reference_asset: payload.variable_payload.min_reference_asset()?,                   // Convert min_reference_asset into Uint128
                    from_amount: payload.variable_payload.from_amount,
                    from_block_number_mod: payload.variable_payload.block_number,
                    calldata_target: parsed_calldata.clone().map(|data| data.target),
                    calldata: parsed_calldata.map(|data| data.bytes)
                })?,
                funds: vec![]
            })

        }
    }?;

    // Build the response 'execute' message
    let sub_msg = SubMsg::reply_always(             // ! Set 'reply_always' so that upon an error of the submessage the 'reply' function of this contract is invoked
        receive_asset_execute_msg,
        RECEIVE_REPLY_ID
    );

    Ok(IbcReceiveResponse::new()        //TODO add attributes?
        .set_ack(ack_success())
        .add_submessage(sub_msg)
    )
}



pub fn on_packet_response(
    deps: DepsMut,
    packet: IbcPacket,
    success: bool
) -> Result<IbcBasicResponse, ContractError> {

    let catalyst_packet = CatalystV1Packet::try_decode(packet.data)?;
    
    // Build the sendAsset/sendLiquidity ack response message
    let receive_asset_execute_msg: cosmwasm_std::WasmMsg = match catalyst_packet {
        CatalystV1Packet::SendAsset(payload) => {

            let from_pool = payload.from_pool_validated(deps.as_ref())?.into_string();  // Validate from_pool   //TODO do we need to validate this?

            // Build execute message
            let msg = match success {
                true => SwapPoolExecuteMsg::<()>::OnSendAssetSuccess {
                    channel_id: packet.dest.channel_id,
                    to_account: payload.to_account.to_binary(),                            // No need to validate, as it must match the one with which the 'swap_hash' was derived
                    u: payload.u,
                    amount: payload.variable_payload.from_amount()?,
                    asset: payload.variable_payload.from_asset_as_string()?,            // No need to validate, as it must match the one with which the 'swap_hash' was derived
                    block_number_mod: payload.variable_payload.block_number
                },
                false => SwapPoolExecuteMsg::<()>::OnSendAssetFailure {
                    channel_id: packet.dest.channel_id,
                    to_account: payload.to_account.to_binary(),                            // No need to validate, as it must match the one with which the 'swap_hash' was derived
                    u: payload.u,
                    amount: payload.variable_payload.from_amount()?,
                    asset: payload.variable_payload.from_asset_as_string()?,            // No need to validate, as it must match the one with which the 'swap_hash' was derived
                    block_number_mod: payload.variable_payload.block_number
                },
            };

            Ok::<cosmwasm_std::WasmMsg, ContractError>(cosmwasm_std::WasmMsg::Execute {
                contract_addr: from_pool,
                msg: to_binary(&msg)?,
                funds: vec![]
            })

        },
        CatalystV1Packet::SendLiquidity(payload) => {

            let from_pool = payload.from_pool_validated(deps.as_ref())?.into_string();  // Validate from_pool   //TODO do we need to validate this?

            // Build execute message
            let msg = match success {
                true => SwapPoolExecuteMsg::<()>::OnSendLiquiditySuccess {
                    channel_id: packet.dest.channel_id,
                    to_account: payload.to_account.to_binary(),                            // No need to validate, as it must match the one with which the 'swap_hash' was derived
                    u: payload.u,
                    amount: payload.variable_payload.from_amount()?,                    // No need to validate, as it must match the one with which the 'swap_hash' was derived
                    block_number_mod: payload.variable_payload.block_number
                },
                false => SwapPoolExecuteMsg::<()>::OnSendLiquidityFailure {
                    channel_id: packet.dest.channel_id,
                    to_account: payload.to_account.to_binary(),                            // No need to validate, as it must match the one with which the 'swap_hash' was derived
                    u: payload.u,
                    amount: payload.variable_payload.from_amount()?,                    // No need to validate, as it must match the one with which the 'swap_hash' was derived
                    block_number_mod: payload.variable_payload.block_number
                },
            };

            Ok::<cosmwasm_std::WasmMsg, ContractError>(cosmwasm_std::WasmMsg::Execute {
                contract_addr: from_pool,
                msg: to_binary(&msg)?,
                funds: vec![]
            })

        }
    }?;

    // Build the 'execute' messsage
    let response_msg = CosmosMsg::Wasm(receive_asset_execute_msg);

    Ok(IbcBasicResponse::new()      //TODO add attributes?
        .add_message(response_msg)
    )
}


pub fn on_packet_success(
    deps: DepsMut,
    packet: IbcPacket
) -> Result<IbcBasicResponse, Never> {
    //TODO The following makes sure packet response processing never fails. Do we want this? If the payload is corrupt (e.g. from_amount > Uint128::MAX), why catch the error?
    on_packet_response(deps, packet, true)
        .or_else(|_| { Ok(IbcBasicResponse::new()) })           //TODO add attributes? (e.g. indicate success ack failed)
}


pub fn on_packet_failure(
    deps: DepsMut,
    packet: IbcPacket
) -> Result<IbcBasicResponse, Never> {
    //TODO The following makes sure packet response processing never fails. Do we want this? If the payload is corrupt (e.g. from_amount > Uint128::MAX), why catch the error?
    on_packet_response(deps, packet, false)
        .or_else(|_| { Ok(IbcBasicResponse::new()) })           //TODO add attributes? (e.g. indicate failed ack/timeout failed)
}

use cosmwasm_std::{
    entry_point, to_binary, Binary, Deps, DepsMut, Env, MessageInfo, Reply, ReplyOn, Response,
    StdResult, SubMsg,
};
use provwasm_std::{create_marker, MarkerType, ProvenanceMsg};

use crate::error::ContractError;
use crate::msg::{InstantiateMsg, QueryMsg};

// Note, you can use StdResult in some functions where you do not
// make use of the custom errors
#[entry_point]
pub fn instantiate(
    _deps: DepsMut,
    env: Env,
    _info: MessageInfo,
    _msg: InstantiateMsg,
) -> Result<Response<ProvenanceMsg>, ContractError> {
    let denom = format!("{}.test", env.contract.address);

    let create = create_marker(1_000_000, denom, MarkerType::Coin)?;

    Ok(Response {
        submessages: vec![create]
            .into_iter()
            .map(|msg| SubMsg {
                id: 100,
                msg,
                gas_limit: None,
                reply_on: ReplyOn::Success,
            })
            .collect(),
        messages: vec![],
        attributes: vec![],
        data: Option::None,
    })
}

#[entry_point]
pub fn reply(_: DepsMut, _env: Env, _: Reply) -> Result<Response, ContractError> {
    Ok(Response::default())
}

#[entry_point]
pub fn query(_deps: Deps, _env: Env, _msg: QueryMsg) -> StdResult<Binary> {
    to_binary("")
}

use cosmwasm_std::{
    entry_point, to_binary, Binary, Deps, DepsMut, Env, MessageInfo, Reply, Response, StdResult,
};
use provwasm_std::{create_marker, MarkerType, ProvenanceMsg};

use crate::error::ContractError;
use crate::msg::{InstantiateMsg, QueryMsg};

#[entry_point]
pub fn instantiate(
    _deps: DepsMut,
    env: Env,
    _info: MessageInfo,
    _msg: InstantiateMsg,
) -> Result<Response<ProvenanceMsg>, ContractError> {
    Ok(Response::new().add_message(create_marker(
        1_000_000,
        format!("{}.test", env.contract.address),
        MarkerType::Coin,
    )?))
}

#[entry_point]
pub fn reply(_: DepsMut, _env: Env, _: Reply) -> Result<Response, ContractError> {
    Ok(Response::default())
}

#[entry_point]
pub fn query(_deps: Deps, _env: Env, _msg: QueryMsg) -> StdResult<Binary> {
    to_binary("")
}

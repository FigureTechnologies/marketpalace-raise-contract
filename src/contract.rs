use cosmwasm_std::{
    entry_point, to_binary, wasm_instantiate, Binary, CosmosMsg, Deps, DepsMut, Env, MessageInfo,
    Reply, ReplyOn, Response, StdResult, SubMsg,
};

use crate::error::ContractError;
use crate::msg::{InstantiateMsg, QueryMsg};

#[entry_point]
pub fn instantiate(
    _deps: DepsMut,
    _env: Env,
    info: MessageInfo,
    msg: InstantiateMsg,
) -> Result<Response<CosmosMsg>, ContractError> {
    Ok(Response {
        submessages: if msg.depth <= 100 {
            vec![SubMsg {
                id: 100,
                msg: wasm_instantiate(
                    1,
                    &InstantiateMsg {
                        depth: msg.depth + 1,
                    },
                    info.funds,
                    format!("{}", msg.depth),
                )?
                .into(),
                gas_limit: None,
                reply_on: ReplyOn::Success,
            }]
        } else {
            vec![]
        },
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

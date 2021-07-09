use crate::msg::CapitalCall;
use cosmwasm_std::{
    entry_point, from_slice, to_binary, wasm_execute, Addr, Binary, CosmosMsg, Deps, DepsMut, Env,
    MessageInfo, Response, StdError, StdResult,
};
use provwasm_std::{
    activate_marker, create_marker, grant_marker_access, MarkerAccess, MarkerType, ProvenanceMsg,
};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use crate::error::ContractError;
use crate::msg::{HandleMsg, InstantiateMsg, QueryMsg};
use crate::state::{config, config_read, State, Status, CONFIG_KEY};

fn contract_error(err: &str) -> ContractError {
    ContractError::Std(StdError::generic_err(err))
}

// Note, you can use StdResult in some functions where you do not
// make use of the custom errors
#[entry_point]
pub fn instantiate(
    deps: DepsMut,
    _env: Env,
    info: MessageInfo,
    msg: InstantiateMsg,
) -> Result<Response<ProvenanceMsg>, ContractError> {
    let state = State {
        status: Status::Active,
        gp: info.sender,
        admin: msg.admin,
        denom: msg.denom.clone(),
        commitment_denom: msg.commitment_denom,
        target: msg.target.clone(),
        min_commitment: msg.min_commitment.clone(),
        max_commitment: msg.max_commitment.clone(),
        pending_capital_promises: vec![],
    };
    config(deps.storage).save(&state)?;

    let create = create_marker(msg.target as u128, msg.denom, MarkerType::Restricted)?;
    let grant = grant_marker_access(
        state.denom.clone(),
        _env.contract.address,
        vec![MarkerAccess::Admin, MarkerAccess::Mint, MarkerAccess::Burn],
    )?;
    let activate = activate_marker(state.denom)?;

    Ok(Response {
        submessages: vec![],
        messages: vec![create, grant, activate],
        attributes: vec![],
        data: Option::None,
    })
}

// And declare a custom Error variant for the ones where you will want to make use of it
#[entry_point]
pub fn execute(
    deps: DepsMut,
    _env: Env,
    info: MessageInfo,
    msg: HandleMsg,
) -> Result<Response<CosmosMsg>, ContractError> {
    match msg {
        HandleMsg::ProposeCapitalPromise {
            capital_promise_address,
        } => try_propose_capital_promise(deps, _env, info, capital_promise_address),
        HandleMsg::Accept {
            promises_and_commitments,
        } => try_accept(deps, _env, info, promises_and_commitments),
    }
}

#[derive(Deserialize)]
pub struct CapitalPromiseState {
    pub owner: Addr,
    pub status: CapitalPromiseStatus,
    pub raise_contract_address: Addr,
    pub admin: Addr,
    pub commitment_denom: String,
    pub min_commitment: u64,
    pub max_commitment: u64,
    pub commitment: Option<u64>,
    pub paid: Option<u64>,
}

#[derive(Deserialize, PartialEq)]
pub enum CapitalPromiseStatus {
    Draft,
    Pending,
    Accepted,
}

#[derive(Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CapitalPromiseMsg {
    SubmitPending {},
    Accept {
        commitment: u64,
    },
    IssueCapitalCall {
        capital_call: CapitalPromiseCapitalCall,
    },
}

#[derive(Serialize)]
pub struct CapitalPromiseCapitalCall {
    pub amount: u64,
    pub days_of_notice: Option<u16>,
}

pub fn try_propose_capital_promise(
    deps: DepsMut,
    _env: Env,
    info: MessageInfo,
    capital_promise_address: Addr,
) -> Result<Response<CosmosMsg>, ContractError> {
    let state = config_read(deps.storage).load()?;

    if state.status != Status::Active {
        return Err(contract_error("contract is not active"));
    }

    let contract: CapitalPromiseState = from_slice(
        &deps
            .querier
            .query_wasm_raw(capital_promise_address.clone(), CONFIG_KEY)?
            .unwrap(),
    )?;

    if contract.owner != info.sender {
        return Err(contract_error(
            "only owner of capital promise can make proposal",
        ));
    }

    if contract.raise_contract_address != _env.contract.address {
        return Err(contract_error(
            "incorrect raise contract address specified on capital promise",
        ));
    }

    if contract.max_commitment < state.min_commitment {
        return Err(contract_error(
            "capital promise max commitment is below raise minumum commitment",
        ));
    }

    if contract.min_commitment > state.max_commitment {
        return Err(contract_error(
            "capital promise min commitment exceeds raise maximum commitment",
        ));
    }

    if contract.status != CapitalPromiseStatus::Draft {
        return Err(contract_error("capital promise not in draft status"));
    }

    config(deps.storage).update(|mut state| -> Result<_, ContractError> {
        state
            .pending_capital_promises
            .push(capital_promise_address.clone());
        Ok(state)
    })?;

    let accept = wasm_execute(
        capital_promise_address,
        &CapitalPromiseMsg::SubmitPending {},
        vec![],
    )?;

    Ok(Response {
        submessages: vec![],
        messages: vec![CosmosMsg::Wasm(accept)],
        attributes: vec![],
        data: Option::None,
    })
}

pub fn try_accept(
    deps: DepsMut,
    _env: Env,
    info: MessageInfo,
    promises_and_commitments: HashMap<Addr, u64>,
) -> Result<Response<CosmosMsg>, ContractError> {
    let state = config_read(deps.storage).load()?;

    Ok(Response {
        submessages: vec![],
        messages: promises_and_commitments
            .into_iter()
            .map(|(promise, commitment)| {
                CosmosMsg::Wasm(
                    wasm_execute(promise, &CapitalPromiseMsg::Accept { commitment }, vec![])
                        .unwrap(),
                )
            })
            .collect(),
        attributes: vec![],
        data: Option::None,
    })
}

pub fn try_issue_capital_calls(
    deps: DepsMut,
    _env: Env,
    info: MessageInfo,
    capital_calls: Vec<CapitalCall>,
) -> Result<Response<CosmosMsg>, ContractError> {
    let state = config_read(deps.storage).load()?;

    Ok(Response {
        submessages: vec![],
        messages: capital_calls
            .into_iter()
            .map(|capital_call| {
                CosmosMsg::Wasm(
                    wasm_execute(
                        capital_call.promise,
                        &CapitalPromiseMsg::IssueCapitalCall {
                            capital_call: CapitalPromiseCapitalCall {
                                amount: capital_call.amount,
                                days_of_notice: None,
                            },
                        },
                        vec![],
                    )
                    .unwrap(),
                )
            })
            .collect(),
        attributes: vec![],
        data: Option::None,
    })
}

#[entry_point]
pub fn query(deps: Deps, _env: Env, msg: QueryMsg) -> StdResult<Binary> {
    match msg {
        QueryMsg::GetStatus {} => to_binary(&query_status(deps)?),
    }
}

fn query_status(deps: Deps) -> StdResult<Status> {
    let state = config_read(deps.storage).load()?;
    Ok(state.status)
}

#[cfg(test)]
mod tests {
    use super::*;
    use cosmwasm_std::testing::{mock_env, mock_info};
    use cosmwasm_std::{coin, coins, from_binary, Addr, Coin, CosmosMsg};
    use provwasm_mocks::{mock_dependencies, must_read_binary_file};
    use provwasm_std::{Marker, MarkerMsgParams, ProvenanceMsgParams};

    fn inst_msg() -> InstantiateMsg {
        InstantiateMsg {
            admin: Addr::unchecked("tp1apnhcu9x5cz2l8hhgnj0hg7ez53jah7hcan000"),
            denom: String::from("funny_money"),
            commitment_denom: String::from("stable_coin"),
            target: 5_000_000,
            min_commitment: 10_000,
            max_commitment: 100_000,
        }
    }

    #[test]
    fn initialization() {
        let mut deps = mock_dependencies(&[]);
        let info = mock_info("creator", &[]);

        // we can just call .unwrap() to assert this was a success
        let res = instantiate(deps.as_mut(), mock_env(), info, inst_msg()).unwrap();
        assert_eq!(3, res.messages.len());

        // it worked, let's query the state
        let res = query(deps.as_ref(), mock_env(), QueryMsg::GetStatus {}).unwrap();
        let status: Status = from_binary(&res).unwrap();
        assert_eq!(Status::Active, status);
    }
}

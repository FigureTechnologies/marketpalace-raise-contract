use cosmwasm_std::{
    entry_point, from_slice, to_binary, wasm_execute, Addr, BankMsg, Binary, Coin, CosmosMsg, Deps,
    DepsMut, Env, MessageInfo, QuerierWrapper, Response, StdError, StdResult,
};
use provwasm_std::{
    create_marker, mint_marker_supply, withdraw_coins, MarkerType, ProvenanceMsg, ProvenanceQuerier,
};
use serde::{Deserialize, Serialize};

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
        status: Status::Proposed,
        gp: info.sender,
        admin: msg.admin,
        denom: msg.denom.clone(),
        target: msg.target.clone(),
        min_commitment: msg.min_commitment.clone(),
        max_commitment: msg.max_commitment.clone(),
    };
    config(deps.storage).save(&state)?;

    if msg.target.denom != msg.min_commitment.denom
        || msg.min_commitment.denom != msg.max_commitment.denom
    {
        return Err(contract_error(
            "denoms do not match between target, min and max commitments",
        ));
    }

    let create = create_marker(msg.target.amount.u128(), msg.denom, MarkerType::Restricted)?;

    Ok(Response {
        submessages: vec![],
        messages: vec![create],
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
        HandleMsg::Activate {} => try_activate(deps, _env, info),
        HandleMsg::ProposeCapitalPromise {
            capital_promise_address,
        } => try_propose_capital_promise(deps, _env, info, capital_promise_address),
    }
}

pub fn try_activate(
    deps: DepsMut,
    _env: Env,
    info: MessageInfo,
) -> Result<Response<CosmosMsg>, ContractError> {
    let state = config_read(deps.storage).load()?;

    if state.status != Status::Proposed {
        return Err(contract_error("contract no longer proposed"));
    }

    if info.sender != state.gp && info.sender != state.admin {
        return Err(contract_error("only gp or admin can activate"));
    }

    config(deps.storage).update(|mut state| -> Result<_, ContractError> {
        state.status = Status::Active;
        Ok(state)
    })?;

    Ok(Response {
        submessages: vec![],
        messages: vec![],
        attributes: vec![],
        data: Option::None,
    })
}

#[derive(Deserialize)]
pub struct CapitalPromiseState {
    pub status: CapitalPromiseStatus,
    pub raise_contract_address: Addr,
    pub admin: Addr,
    pub min_commitment: Coin,
    pub max_commitment: Coin,
}

#[derive(Deserialize, PartialEq)]
pub enum CapitalPromiseStatus {
    Proposed,
    ContractAccepted,
    GPAccepted,
}

#[derive(Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CapitalPromiseMsg {
    ContractAccept {},
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

    if contract.raise_contract_address != _env.contract.address {
        return Err(contract_error(
            "incorrect raise contract address specified on capital promise",
        ));
    }

    if contract.min_commitment.denom != state.target.denom {
        return Err(contract_error(
            "commitment denom doesn't match target denom",
        ));
    }

    if contract.max_commitment.amount < state.min_commitment.amount {
        return Err(contract_error(
            "capital promise max commitment is below raise minumum commitment",
        ));
    }

    if contract.min_commitment.amount > state.max_commitment.amount {
        return Err(contract_error(
            "capital promise min commitment exceeds raise maximum commitment",
        ));
    }

    if contract.status != CapitalPromiseStatus::Proposed {
        return Err(contract_error("capital promise not in proposed status"));
    }

    let accept = wasm_execute(
        capital_promise_address,
        &CapitalPromiseMsg::ContractAccept {},
        vec![],
    )?;

    Ok(Response {
        submessages: vec![],
        messages: vec![CosmosMsg::Wasm(accept)],
        attributes: vec![],
        data: Option::None,
    })
}

// pub fn try_commit_capital(
//     deps: DepsMut,
//     _env: Env,
//     info: MessageInfo,
// ) -> Result<Response<ProvenanceMsg>, ContractError> {
//     let state = config_read(deps.storage).load()?;

//     if state.status != Status::PendingCapital {
//         return Err(contract_error("contract no longer pending capital"));
//     }

//     if info.sender != state.lp_capital_source {
//         return Err(contract_error("wrong investor committing capital"));
//     }

//     if info.funds.is_empty() {
//         return Err(contract_error("no capital was committed"));
//     }

//     let deposit = info.funds.first().unwrap();
//     if deposit != &state.capital {
//         return Err(contract_error("capital does not match required"));
//     }

//     config(deps.storage).update(|mut state| -> Result<_, ContractError> {
//         state.status = Status::CapitalCommitted;
//         Ok(state)
//     })?;

//     Ok(Response {
//         submessages: vec![],
//         messages: vec![],
//         attributes: vec![],
//         data: Option::None,
//     })
// }

// pub fn try_cancel(
//     deps: DepsMut,
//     _env: Env,
//     info: MessageInfo,
// ) -> Result<Response<ProvenanceMsg>, ContractError> {
//     let state = config_read(deps.storage).load()?;

//     if state.status == Status::CapitalCalled {
//         return Err(contract_error("capital already called"));
//     } else if state.status == Status::Cancelled {
//         return Err(contract_error("already cancelled"));
//     }

//     if info.sender != state.gp && info.sender != state.admin {
//         return Err(contract_error("wrong gp cancelling capital call"));
//     }

//     config(deps.storage).update(|mut state| -> Result<_, ContractError> {
//         state.status = Status::Cancelled;
//         Ok(state)
//     })?;

//     Ok(Response {
//         submessages: vec![],
//         messages: if state.status == Status::CapitalCommitted {
//             vec![BankMsg::Send {
//                 to_address: state.lp_capital_source.to_string(),
//                 amount: vec![state.capital],
//             }
//             .into()]
//         } else {
//             vec![]
//         },
//         attributes: vec![],
//         data: Option::None,
//     })
// }

// pub fn try_call_capital(
//     deps: DepsMut,
//     _env: Env,
//     info: MessageInfo,
// ) -> Result<Response<ProvenanceMsg>, ContractError> {
//     let state = config_read(deps.storage).load()?;

//     if state.status != Status::CapitalCommitted {
//         return Err(contract_error("capital not committed"));
//     }

//     if info.sender != state.gp && info.sender != state.admin {
//         return Err(contract_error("wrong gp calling capital"));
//     }

//     config(deps.storage).update(|mut state| -> Result<_, ContractError> {
//         state.status = Status::CapitalCalled;
//         Ok(state)
//     })?;

//     let mint = mint_marker_supply(state.shares.amount.into(), state.shares.denom.clone())?;
//     let withdraw = withdraw_coins(
//         state.shares.denom.clone(),
//         state.shares.amount.into(),
//         state.shares.denom.clone(),
//         state.lp_capital_source,
//     )?;

//     let marker = ProvenanceQuerier::new(&deps.querier).get_marker_by_denom(state.shares.denom)?;

//     Ok(Response {
//         submessages: vec![],
//         messages: vec![
//             mint,
//             withdraw,
//             BankMsg::Send {
//                 to_address: marker.address.to_string(),
//                 amount: vec![state.capital],
//             }
//             .into(),
//         ],
//         attributes: vec![],
//         data: Option::None,
//     })
// }

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
            target: coin(5_000_000, "stable_coin"),
            min_commitment: coin(10_000, "stable_coin"),
            max_commitment: coin(100_000, "stable_coin"),
        }
    }

    #[test]
    fn initialization() {
        let mut deps = mock_dependencies(&[]);
        let info = mock_info("creator", &[]);

        // we can just call .unwrap() to assert this was a success
        let res = instantiate(deps.as_mut(), mock_env(), info, inst_msg()).unwrap();
        assert_eq!(1, res.messages.len());

        // it worked, let's query the state
        let res = query(deps.as_ref(), mock_env(), QueryMsg::GetStatus {}).unwrap();
        let status: Status = from_binary(&res).unwrap();
        assert_eq!(Status::Proposed, status);
    }

    // #[test]
    // fn commit_capital() {
    //     let mut deps = mock_dependencies(&coins(2, "token"));

    //     let info = mock_info("creator", &[]);
    //     let _res = instantiate(deps.as_mut(), mock_env(), info, inst_msg()).unwrap();

    //     // lp can commit capital
    //     let info = mock_info(
    //         "tp18lysxk7sueunnspju4dar34vlv98a7kyyfkqs7",
    //         &coins(1000000, "cfigure"),
    //     );
    //     let msg = HandleMsg::CommitCapital {};
    //     let _res = execute(deps.as_mut(), mock_env(), info, msg).unwrap();

    //     // should be in capital commited state
    //     let res = query(deps.as_ref(), mock_env(), QueryMsg::GetStatus {}).unwrap();
    //     let status: Status = from_binary(&res).unwrap();
    //     assert_eq!(Status::CapitalCommitted, status);
    // }

    // #[test]
    // fn cancel() {
    //     let mut deps = mock_dependencies(&coins(2, "token"));

    //     let info = mock_info("creator", &[]);
    //     let _res = instantiate(deps.as_mut(), mock_env(), info, inst_msg()).unwrap();

    //     // lp can commit capital
    //     let info = mock_info(
    //         "tp18lysxk7sueunnspju4dar34vlv98a7kyyfkqs7",
    //         &coins(1000000, "cfigure"),
    //     );
    //     let msg = HandleMsg::CommitCapital {};
    //     let _res = execute(deps.as_mut(), mock_env(), info, msg).unwrap();

    //     // gp can cancel capital call
    //     let info = mock_info("creator", &[]);
    //     let msg = HandleMsg::Cancel {};
    //     let _res = execute(deps.as_mut(), mock_env(), info, msg).unwrap();

    //     // should be in pending capital state
    //     let res = query(deps.as_ref(), mock_env(), QueryMsg::GetStatus {}).unwrap();
    //     let status: Status = from_binary(&res).unwrap();
    //     assert_eq!(Status::Cancelled, status);

    //     // should send stable coin back to lp
    //     let (to_address, amount) = _res
    //         .messages
    //         .iter()
    //         .find_map(|msg| match msg {
    //             CosmosMsg::Bank(bank) => match bank {
    //                 BankMsg::Send { to_address, amount } => Some((to_address, amount)),
    //                 _ => None,
    //             },
    //             _ => None,
    //         })
    //         .unwrap();
    //     assert_eq!("tp18lysxk7sueunnspju4dar34vlv98a7kyyfkqs7", to_address);
    //     assert_eq!(1000000, u128::from(amount[0].amount));
    //     assert_eq!("cfigure", amount[0].denom);
    // }

    // #[test]
    // fn call_capital() {
    //     // Create a mock querier with our expected marker.
    //     let bin = must_read_binary_file("testdata/marker.json");
    //     let expected_marker: Marker = from_binary(&bin).unwrap();
    //     let mut deps = mock_dependencies(&[]);
    //     deps.querier.with_markers(vec![expected_marker.clone()]);

    //     let info = mock_info("creator", &[]);
    //     let _res = instantiate(deps.as_mut(), mock_env(), info, inst_msg()).unwrap();

    //     // lp can commit capital
    //     let info = mock_info(
    //         "tp18lysxk7sueunnspju4dar34vlv98a7kyyfkqs7",
    //         &coins(1000000, "cfigure"),
    //     );
    //     let msg = HandleMsg::CommitCapital {};
    //     let _res = execute(deps.as_mut(), mock_env(), info, msg).unwrap();

    //     // gp can call capital
    //     let info = mock_info("creator", &vec![]);
    //     let msg = HandleMsg::CallCapital {};
    //     let _res = execute(deps.as_mut(), mock_env(), info, msg).unwrap();

    //     let mint = _res
    //         .messages
    //         .iter()
    //         .find_map(|msg| match msg {
    //             CosmosMsg::Custom(custom) => match custom {
    //                 ProvenanceMsg {
    //                     route: _,
    //                     params,
    //                     version: _,
    //                 } => match params {
    //                     ProvenanceMsgParams::Marker(params) => match params {
    //                         MarkerMsgParams::MintMarkerSupply { coin } => Some(coin),
    //                         _ => None,
    //                     },
    //                     _ => None,
    //                 },
    //             },
    //             _ => None,
    //         })
    //         .unwrap();
    //     assert_eq!(10, u128::from(mint.amount));

    //     let (withdraw_coin, withdraw_recipient) = _res
    //         .messages
    //         .iter()
    //         .find_map(|msg| match msg {
    //             CosmosMsg::Custom(custom) => match custom {
    //                 ProvenanceMsg {
    //                     route: _,
    //                     params,
    //                     version: _,
    //                 } => match params {
    //                     ProvenanceMsgParams::Marker(params) => match params {
    //                         MarkerMsgParams::WithdrawCoins {
    //                             marker_denom: _,
    //                             coin,
    //                             recipient,
    //                         } => Some((coin, recipient)),
    //                         _ => None,
    //                     },
    //                     _ => None,
    //                 },
    //             },
    //             _ => None,
    //         })
    //         .unwrap();
    //     assert_eq!(10, u128::from(withdraw_coin.amount));
    //     assert_eq!(
    //         "tp18lysxk7sueunnspju4dar34vlv98a7kyyfkqs7",
    //         withdraw_recipient.to_string()
    //     );

    //     let (to_address, amount) = _res
    //         .messages
    //         .iter()
    //         .find_map(|msg| match msg {
    //             CosmosMsg::Bank(bank) => match bank {
    //                 BankMsg::Send { to_address, amount } => Some((to_address, amount)),
    //                 _ => None,
    //             },
    //             _ => None,
    //         })
    //         .unwrap();
    //     assert_eq!("tp18vmzryrvwaeykmdtu6cfrz5sau3dhc5c73ms0u", to_address);
    //     assert_eq!(1000000, u128::from(amount[0].amount));
    //     assert_eq!("cfigure", amount[0].denom);

    //     // should be in capital called state
    //     let res = query(deps.as_ref(), mock_env(), QueryMsg::GetStatus {}).unwrap();
    //     let status: Status = from_binary(&res).unwrap();
    //     assert_eq!(Status::CapitalCalled, status);
    // }
}

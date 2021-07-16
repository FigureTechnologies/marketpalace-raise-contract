use cosmwasm_std::{
    coin, entry_point, from_slice, to_binary, wasm_execute, Addr, BankMsg,
    Binary, Coin, CosmosMsg, Deps, DepsMut, Env, MessageInfo, Response, StdError, StdResult,
};
use provwasm_std::{
    activate_marker, create_marker, grant_marker_access, MarkerAccess, MarkerType, ProvenanceMsg,
    ProvenanceQuerier,
};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::collections::HashSet;

use crate::error::ContractError;
use crate::msg::{HandleMsg, InstantiateMsg, QueryMsg, Subs};
use crate::state::{config, config_read, State, Status, CONFIG_KEY};
use crate::sub::{SubExecuteMsg, SubQueryMsg, SubTerms};

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
        capital_call_code_id: msg.capital_call_code_id,
        gp: info.sender,
        admin: msg.admin,
        qualified_tags: msg.qualified_tags,
        asset_denom: msg.asset_denom.clone(),
        capital_denom: msg.capital_denom,
        target: msg.target.clone(),
        min_commitment: msg.min_commitment.clone(),
        max_commitment: msg.max_commitment.clone(),
        pending_review_subs: HashSet::new(),
        accepted_subs: HashSet::new(),
        capital_calls: HashSet::new(),
    };
    config(deps.storage).save(&state)?;

    let create = create_marker(msg.target as u128, msg.asset_denom, MarkerType::Restricted)?;
    let grant = grant_marker_access(
        state.asset_denom.clone(),
        _env.contract.address,
        vec![MarkerAccess::Admin, MarkerAccess::Mint, MarkerAccess::Burn],
    )?;
    let activate = activate_marker(state.asset_denom)?;

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
    env: Env,
    info: MessageInfo,
    msg: HandleMsg,
) -> Result<Response<ProvenanceMsg>, ContractError> {
    match msg {
        HandleMsg::ProposeSubscription { subscription } => {
            try_propose_subscription(deps, env, info, subscription)
        }
        HandleMsg::AcceptSubscriptions { subscriptions } => {
            try_accept_subscriptions(deps, info, subscriptions)
        }
        HandleMsg::IssueCalls { calls } => try_issue_calls(deps, info, calls),
        HandleMsg::CloseCalls { calls } => try_close_calls(deps, info, calls),
        HandleMsg::IssueDistributions { distributions } => {
            try_issue_distributions(deps, info, distributions)
        }
        HandleMsg::RedeemCapital { to, amount, memo } => {
            try_redeem_capital(deps, info, to, amount, memo)
        }
    }
}

#[derive(Deserialize)]
pub struct CapitalCallState {
    pub subscription: Addr,
    pub amount: u64,
}

#[derive(Serialize)]
#[serde(rename_all = "snake_case")]
pub struct InstantiateCapitalCallMsg {
    pub subscription: Addr,
    pub capital: Coin,
    pub asset: Coin,
}

#[derive(Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CapitalCallMsg {
    Close {},
}

pub fn try_propose_subscription(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    subscription: Addr,
) -> Result<Response<ProvenanceMsg>, ContractError> {
    let state = config_read(deps.storage).load()?;

    if state.status != Status::Active {
        return Err(contract_error("contract is not active"));
    }

    let terms: SubTerms = deps
        .querier
        .query_wasm_smart(subscription.clone(), &SubQueryMsg::GetTerms {}).expect("terms");

    if terms.owner != info.sender {
        return Err(contract_error(
            "only owner of subscription can make proposal",
        ));
    }

    if terms.raise != env.contract.address {
        return Err(contract_error(
            "incorrect raise contract address specified on subscription",
        ));
    }

    if terms.capital_denom != state.capital_denom {
        return Err(contract_error("both sub and raise need to have the same capital denom"));
    }

    if terms.max_commitment < state.min_commitment {
        return Err(contract_error(
            "capital promise max commitment is below raise minumum commitment",
        ));
    }

    if terms.min_commitment > state.max_commitment {
        return Err(contract_error(
            "capital promise min commitment exceeds raise maximum commitment",
        ));
    }

    if !state.qualified_tags.is_empty() {
        let attributes = ProvenanceQuerier::new(&deps.querier)
            .get_attributes(terms.owner, None as Option<String>)?
            .attributes;

        if !attributes
            .iter()
            .any(|attribute| state.qualified_tags.contains(&attribute.name))
        {
            return Err(contract_error(
                "subscription owner must have one of qualified tages",
            ));
        }
    }

    config(deps.storage).update(|mut state| -> Result<_, ContractError> {
        state.pending_review_subs.insert(subscription.clone());
        Ok(state)
    })?;

    Ok(Response {
        submessages: vec![],
        messages: vec![],
        attributes: vec![],
        data: Option::None,
    })
}

pub fn try_accept_subscriptions(
    deps: DepsMut,
    info: MessageInfo,
    subscriptions: HashMap<Addr, u64>,
) -> Result<Response<ProvenanceMsg>, ContractError> {
    let state = config_read(deps.storage).load()?;

    if info.sender != state.gp && info.sender != state.admin {
        return Err(contract_error("only gp or admin can accept subscriptions"));
    }

    config(deps.storage).update(|mut state| -> Result<_, ContractError> {
        subscriptions.iter().for_each(|(sub, _)| {
            state.pending_review_subs.remove(sub);
            state.accepted_subs.insert(sub.clone());
        });

        Ok(state)
    })?;

    Ok(Response {
        submessages: vec![],
        messages: subscriptions
            .into_iter()
            .map(|(subscription, commitment)| {
                CosmosMsg::Wasm(
                    wasm_execute(subscription, &SubExecuteMsg::Accept { commitment }, vec![])
                        .unwrap(),
                )
            })
            .collect(),
        attributes: vec![],
        data: Option::None,
    })
}

pub fn try_issue_calls(
    deps: DepsMut,
    info: MessageInfo,
    calls: HashSet<Addr>,
) -> Result<Response<ProvenanceMsg>, ContractError> {
    let state = config_read(deps.storage).load()?;

    if info.sender != state.gp && info.sender != state.admin {
        return Err(contract_error("only gp or admin can issue calls"));
    }

    config(deps.storage).update(|mut state| -> Result<_, ContractError> {
        state.capital_calls = state
            .capital_calls
            .union(&calls)
            .map(|it| it.clone())
            .collect();
        Ok(state)
    })?;

    let calls = calls
        .into_iter()
        .flat_map(|call| {
            let contract: CapitalCallState = from_slice(
                &deps
                    .querier
                    .query_wasm_raw(call.clone(), CONFIG_KEY)
                    .unwrap()
                    .unwrap(),
            )
            .unwrap();

            let grant = grant_marker_access(
                state.asset_denom.clone(),
                call.clone(),
                vec![MarkerAccess::Withdraw],
            )
            .unwrap();

            let issue = CosmosMsg::Wasm(
                wasm_execute(
                    contract.subscription,
                    &SubExecuteMsg::IssueCapitalCall { capital_call: call },
                    vec![],
                )
                .unwrap(),
            );

            vec![grant, issue]
        })
        .collect();

    Ok(Response {
        submessages: vec![],
        messages: calls,
        attributes: vec![],
        data: Option::None,
    })
}

pub fn try_close_calls(
    deps: DepsMut,
    info: MessageInfo,
    calls: Vec<Addr>,
) -> Result<Response<ProvenanceMsg>, ContractError> {
    let state = config_read(deps.storage).load()?;

    if info.sender != state.gp && info.sender != state.admin {
        return Err(contract_error("only gp or admin can close calls"));
    }

    let close_messages = calls
        .into_iter()
        .map(|call| CosmosMsg::Wasm(wasm_execute(call, &CapitalCallMsg::Close {}, vec![]).unwrap()))
        .collect();

    Ok(Response {
        submessages: vec![],
        messages: close_messages,
        attributes: vec![],
        data: Option::None,
    })
}

pub fn try_issue_distributions(
    deps: DepsMut,
    info: MessageInfo,
    distributions: HashMap<Addr, u64>,
) -> Result<Response<ProvenanceMsg>, ContractError> {
    let state = config_read(deps.storage).load()?;

    if info.sender != state.gp && info.sender != state.admin {
        return Err(contract_error("only gp or admin can issue distributions"));
    }

    let capital = info.funds.first().unwrap();
    if capital.denom != state.capital_denom {
        return Err(contract_error("incorrect capital denom"));
    }

    let total = distributions.iter().fold(0, |sum, next| sum + next.1);
    if capital.amount.u128() as u64 != total {
        return Err(contract_error(
            "incorrect capital sent for all distributions",
        ));
    }

    let distributions = distributions
        .into_iter()
        .map(|(subscription, distribution)| {
            CosmosMsg::Wasm(
                wasm_execute(
                    subscription,
                    &SubExecuteMsg::IssueDistribution {},
                    vec![coin(distribution as u128, state.capital_denom.clone())],
                )
                .unwrap(),
            )
        })
        .collect();

    Ok(Response {
        submessages: vec![],
        messages: distributions,
        attributes: vec![],
        data: Option::None,
    })
}

pub fn try_redeem_capital(
    deps: DepsMut,
    info: MessageInfo,
    to: Addr,
    amount: u64,
    memo: Option<String>,
) -> Result<Response<ProvenanceMsg>, ContractError> {
    let state = config_read(deps.storage).load()?;

    if info.sender != state.gp && info.sender != state.admin {
        return Err(contract_error("only gp can redeem capital"));
    }

    let send = BankMsg::Send {
        to_address: to.to_string(),
        amount: vec![coin(amount as u128, state.capital_denom)],
    }
    .into();

    Ok(Response {
        submessages: vec![],
        messages: vec![send],
        attributes: vec![],
        data: Option::None,
    })
}

#[entry_point]
pub fn query(deps: Deps, _env: Env, msg: QueryMsg) -> StdResult<Binary> {
    let state = config_read(deps.storage).load()?;

    match msg {
        QueryMsg::GetStatus {} => to_binary(&state.status),
        QueryMsg::GetSubs {} => to_binary(&Subs{
            pending_review: state.pending_review_subs,
            accepted: state.accepted_subs,
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use cosmwasm_std::testing::{mock_env, mock_info, mock_dependencies, MockStorage, MockApi, MOCK_CONTRACT_ADDR};
    use cosmwasm_std::{coin, coins, from_binary, Addr, Coin, ContractResult, CosmosMsg, CustomQuery, Empty, OwnedDeps, Querier, QueryRequest, WasmQuery, SystemResult, SystemError};
    use crate::mock::MockContractQuerier;
    use provwasm_mocks::{must_read_binary_file};
    use provwasm_std::{Marker, MarkerMsgParams, ProvenanceMsgParams};

    fn inst_msg() -> InstantiateMsg {
        InstantiateMsg {
            capital_call_code_id: 117,
            admin: Addr::unchecked("tp1apnhcu9x5cz2l8hhgnj0hg7ez53jah7hcan000"),
            qualified_tags: vec![],
            asset_denom: String::from("funny_money"),
            capital_denom: String::from("stable_coin"),
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

    #[test]
    fn try_propose_subscription() {
        let mut deps = OwnedDeps {
            storage: MockStorage::default(),
            api: MockApi::default(),
            querier: MockContractQuerier {
                wasm_smart_handler: |_contract_addr, _msg| {
                    SystemResult::Ok(ContractResult::Ok(to_binary(&SubTerms {
                        owner: Addr::unchecked("lp"),
                        raise: Addr::unchecked(MOCK_CONTRACT_ADDR),
                        capital_denom: String::from("stable_coin"),
                        min_commitment: 10_000,
                        max_commitment: 50_000,
                    }).unwrap()))
                },
            },
        };

        // we can just call .unwrap() to assert this was a success
        instantiate(
            deps.as_mut(),
            mock_env(),
            mock_info("creator", &[]),
            inst_msg(),
        )
        .unwrap();

        execute(
            deps.as_mut(),
            mock_env(),
            mock_info("lp", &[]),
            HandleMsg::ProposeSubscription {
                subscription: Addr::unchecked("sub"),
            },
        )
        .unwrap();

        let res = query(deps.as_ref(), mock_env(), QueryMsg::GetSubs {}).unwrap();
        let subs: Subs = from_binary(&res).unwrap();
        assert_eq!(1, subs.pending_review.len());
    }
}

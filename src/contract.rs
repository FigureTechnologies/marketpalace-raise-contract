use cosmwasm_std::{
    coin, coins, entry_point, to_binary, wasm_execute, Addr, Attribute, BankMsg, Binary, CosmosMsg,
    Deps, DepsMut, Env, MessageInfo, Response, StdError, StdResult,
};
use provwasm_std::{
    activate_marker, create_marker, grant_marker_access, MarkerAccess, MarkerType, ProvenanceMsg,
    ProvenanceQuerier,
};
use std::collections::HashMap;
use std::collections::HashSet;

use crate::call::{CallMsg, CallQueryMsg, CallTerms};
use crate::error::ContractError;
use crate::msg::{Calls, HandleMsg, InstantiateMsg, QueryMsg, Redemption, Subs, Terms};
use crate::state::{config, config_read, State, Status};
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
        gp: info.sender,
        admin: msg.admin,
        qualified_tags: msg.qualified_tags,
        asset_denom: msg.asset_denom.clone(),
        capital_denom: msg.capital_denom,
        target: msg.target,
        min_commitment: msg.min_commitment,
        max_commitment: msg.max_commitment,
        pending_review_subs: HashSet::new(),
        accepted_subs: HashSet::new(),
        issued_calls: HashSet::new(),
        closed_calls: HashSet::new(),
    };
    config(deps.storage).save(&state)?;

    let create = create_marker(msg.target as u128, msg.asset_denom, MarkerType::Restricted)?;
    let grant = grant_marker_access(
        state.asset_denom.clone(),
        _env.contract.address,
        vec![
            MarkerAccess::Admin,
            MarkerAccess::Mint,
            MarkerAccess::Burn,
            MarkerAccess::Withdraw,
        ],
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
        HandleMsg::Recover { gp } => try_recover(deps, info, gp),
        HandleMsg::ProposeSubscription { subscription } => {
            try_propose_subscription(deps, env, info, subscription)
        }
        HandleMsg::AcceptSubscriptions { subscriptions } => {
            try_accept_subscriptions(deps, info, subscriptions)
        }
        HandleMsg::IssueCalls { calls } => try_issue_calls(deps, info, calls),
        HandleMsg::CloseCalls { calls } => try_close_calls(deps, info, calls),
        HandleMsg::IssueRedemptions { redemptions } => {
            try_issue_redemptions(deps, info, redemptions)
        }
        HandleMsg::IssueDistributions { distributions } => {
            try_issue_distributions(deps, info, distributions)
        }
        HandleMsg::RedeemCapital { to, amount, memo } => {
            try_redeem_capital(deps, info, to, amount, memo)
        }
    }
}

pub fn try_recover(
    deps: DepsMut,
    info: MessageInfo,
    gp: Addr,
) -> Result<Response<ProvenanceMsg>, ContractError> {
    let state = config_read(deps.storage).load()?;

    if info.sender != state.admin {
        return Err(contract_error("only admin can recover raise"));
    }

    config(deps.storage).update(|mut state| -> Result<_, ContractError> {
        state.gp = gp;
        Ok(state)
    })?;

    Ok(Response {
        submessages: vec![],
        messages: vec![],
        attributes: vec![],
        data: Option::None,
    })
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
        .query_wasm_smart(subscription.clone(), &SubQueryMsg::GetTerms {})
        .expect("terms");

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
        return Err(contract_error(
            "both sub and raise need to have the same capital denom",
        ));
    }

    if let Some(min) = state.min_commitment {
        if terms.max_commitment < min {
            return Err(contract_error(
                "capital promise max commitment is below raise minumum commitment",
            ));
        }
    }

    if let Some(max) = state.max_commitment {
        if terms.min_commitment > max {
            return Err(contract_error(
                "capital promise min commitment exceeds raise maximum commitment",
            ));
        }
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

    if info.sender != state.gp {
        return Err(contract_error("only gp can accept subscriptions"));
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
                    wasm_execute(
                        subscription,
                        &SubExecuteMsg::Accept {},
                        coins(commitment as u128, state.asset_denom.clone()),
                    )
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

    if info.sender != state.gp {
        return Err(contract_error("only gp can issue calls"));
    }

    config(deps.storage).update(|mut state| -> Result<_, ContractError> {
        state.issued_calls = state.issued_calls.union(&calls).cloned().collect();
        Ok(state)
    })?;

    let calls = calls
        .into_iter()
        .flat_map(|call| {
            let terms: CallTerms = deps
                .querier
                .query_wasm_smart(call.clone(), &CallQueryMsg::GetTerms {})
                .expect("terms");

            let grant = grant_marker_access(
                state.asset_denom.clone(),
                call.clone(),
                vec![MarkerAccess::Withdraw],
            )
            .unwrap();

            let issue = CosmosMsg::Wasm(
                wasm_execute(
                    terms.subscription,
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

    if info.sender != state.gp {
        return Err(contract_error("only gp can close calls"));
    }

    config(deps.storage).update(|mut state| -> Result<_, ContractError> {
        calls.iter().for_each(|call| {
            state.issued_calls.remove(call);
            state.closed_calls.insert(call.clone());
        });

        Ok(state)
    })?;

    let close_messages = calls
        .into_iter()
        .map(|call| {
            let terms: CallTerms = deps
                .querier
                .query_wasm_smart(call.clone(), &CallQueryMsg::GetTerms {})
                .expect("terms");

            CosmosMsg::Wasm(
                wasm_execute(
                    call,
                    &CallMsg::Close {},
                    coins(
                        terms.amount as u128,
                        format!("commitment_{}_{}", terms.subscription, terms.raise),
                    ),
                )
                .unwrap(),
            )
        })
        .collect();

    Ok(Response {
        submessages: vec![],
        messages: close_messages,
        attributes: vec![],
        data: Option::None,
    })
}

pub fn try_issue_redemptions(
    deps: DepsMut,
    info: MessageInfo,
    redemptions: HashSet<Redemption>,
) -> Result<Response<ProvenanceMsg>, ContractError> {
    let state = config_read(deps.storage).load()?;

    if info.sender != state.gp {
        return Err(contract_error("only gp can issue redemptions"));
    }

    let capital = info.funds.first().unwrap();
    if capital.denom != state.capital_denom {
        return Err(contract_error("incorrect capital denom"));
    }

    let total = redemptions.iter().fold(0, |sum, next| sum + next.capital);
    if capital.amount.u128() as u64 != total {
        return Err(contract_error("incorrect capital sent for all redemptions"));
    }

    let redemptions = redemptions
        .into_iter()
        .map(|redemption| {
            CosmosMsg::Wasm(
                wasm_execute(
                    redemption.subscription,
                    &SubExecuteMsg::IssueRedemption {
                        redemption: redemption.asset,
                    },
                    vec![coin(
                        redemption.capital as u128,
                        state.capital_denom.clone(),
                    )],
                )
                .unwrap(),
            )
        })
        .collect();

    Ok(Response {
        submessages: vec![],
        messages: redemptions,
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

    if info.sender != state.gp {
        return Err(contract_error("only gp can issue distributions"));
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

    if info.sender != state.gp {
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
        attributes: match memo {
            Some(memo) => {
                vec![Attribute {
                    key: String::from("memo"),
                    value: memo,
                }]
            }
            None => vec![],
        },
        data: Option::None,
    })
}

#[entry_point]
pub fn query(deps: Deps, _env: Env, msg: QueryMsg) -> StdResult<Binary> {
    let state = config_read(deps.storage).load()?;

    match msg {
        QueryMsg::GetStatus {} => to_binary(&state.status),
        QueryMsg::GetTerms {} => to_binary(&Terms {
            qualified_tags: state.qualified_tags,
            asset_denom: state.asset_denom,
            capital_denom: state.capital_denom,
            target: state.target,
            min_commitment: state.min_commitment,
            max_commitment: state.max_commitment,
        }),
        QueryMsg::GetSubs {} => to_binary(&Subs {
            pending_review: state.pending_review_subs,
            accepted: state.accepted_subs,
        }),
        QueryMsg::GetCalls {} => to_binary(&Calls {
            issued: state.issued_calls,
            closed: state.closed_calls,
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::mock::wasm_smart_mock_dependencies;
    use cosmwasm_std::testing::{mock_dependencies, mock_env, mock_info, MOCK_CONTRACT_ADDR};
    use cosmwasm_std::{from_binary, Addr, ContractResult, SystemError, SystemResult};

    #[test]
    fn initialization() {
        let mut deps = mock_dependencies(&[]);
        let info = mock_info("gp", &[]);

        // instantiate and verify we have 3 messages (create, grant, & activate)
        let res = instantiate(
            deps.as_mut(),
            mock_env(),
            info,
            InstantiateMsg {
                admin: Addr::unchecked("marketpalace"),
                qualified_tags: vec![],
                asset_denom: String::from("fund_coin"),
                capital_denom: String::from("stable_coin"),
                target: 5_000_000,
                min_commitment: Some(10_000),
                max_commitment: Some(100_000),
            },
        )
        .unwrap();
        assert_eq!(3, res.messages.len());

        // verify raise is in active status
        let res = query(deps.as_ref(), mock_env(), QueryMsg::GetStatus {}).unwrap();
        let status: Status = from_binary(&res).unwrap();
        assert_eq!(Status::Active, status);

        // verify that terms of raise are correct
        let res = query(deps.as_ref(), mock_env(), QueryMsg::GetTerms {}).unwrap();
        let terms: Terms = from_binary(&res).unwrap();
        assert_eq!(0, terms.qualified_tags.len());
        assert_eq!("fund_coin", terms.asset_denom);
        assert_eq!("stable_coin", terms.capital_denom);
        assert_eq!(5_000_000, terms.target);
        assert_eq!(10_000, terms.min_commitment.unwrap());
        assert_eq!(100_000, terms.max_commitment.unwrap());
    }

    #[test]
    fn recover() {
        let mut deps = mock_dependencies(&vec![]);

        config(&mut deps.storage)
            .save(&State {
                status: Status::Active,
                gp: Addr::unchecked("gp"),
                admin: Addr::unchecked("marketpalace"),
                qualified_tags: vec![],
                asset_denom: String::from("fund_coin"),
                capital_denom: String::from("stable_coin"),
                target: 5_000_000,
                min_commitment: Some(10_000),
                max_commitment: Some(100_000),
                pending_review_subs: HashSet::new(),
                accepted_subs: HashSet::new(),
                issued_calls: HashSet::new(),
                closed_calls: HashSet::new(),
            })
            .unwrap();

        execute(
            deps.as_mut(),
            mock_env(),
            mock_info("marketpalace", &vec![]),
            HandleMsg::Recover {
                gp: Addr::unchecked("gp_2"),
            },
        )
        .unwrap();
    }

    #[test]
    fn fail_bad_actor_recover() {
        let mut deps = mock_dependencies(&vec![]);

        config(&mut deps.storage)
            .save(&State {
                status: Status::Active,
                gp: Addr::unchecked("gp"),
                admin: Addr::unchecked("marketpalace"),
                qualified_tags: vec![],
                asset_denom: String::from("fund_coin"),
                capital_denom: String::from("stable_coin"),
                target: 5_000_000,
                min_commitment: Some(10_000),
                max_commitment: Some(100_000),
                pending_review_subs: HashSet::new(),
                accepted_subs: HashSet::new(),
                issued_calls: HashSet::new(),
                closed_calls: HashSet::new(),
            })
            .unwrap();

        let res = execute(
            deps.as_mut(),
            mock_env(),
            mock_info("bad_actor", &vec![]),
            HandleMsg::Recover {
                gp: Addr::unchecked("bad_actor"),
            },
        );
        assert_eq!(true, res.is_err());
    }

    #[test]
    fn propose_subscription() {
        let mut deps =
            wasm_smart_mock_dependencies(&vec![], |contract_addr, _msg| match &contract_addr[..] {
                "sub_1" => SystemResult::Ok(ContractResult::Ok(
                    to_binary(&SubTerms {
                        owner: Addr::unchecked("lp"),
                        raise: Addr::unchecked(MOCK_CONTRACT_ADDR),
                        capital_denom: String::from("stable_coin"),
                        min_commitment: 10_000,
                        max_commitment: 50_000,
                    })
                    .unwrap(),
                )),
                _ => SystemResult::Err(SystemError::UnsupportedRequest {
                    kind: String::from("not mocked"),
                }),
            });

        config(&mut deps.storage)
            .save(&State {
                status: Status::Active,
                gp: Addr::unchecked("gp"),
                admin: Addr::unchecked("marketpalace"),
                qualified_tags: vec![],
                asset_denom: String::from("fund_coin"),
                capital_denom: String::from("stable_coin"),
                target: 5_000_000,
                min_commitment: Some(10_000),
                max_commitment: Some(100_000),
                pending_review_subs: HashSet::new(),
                accepted_subs: HashSet::new(),
                issued_calls: HashSet::new(),
                closed_calls: HashSet::new(),
            })
            .unwrap();

        // propose a sub as lp
        execute(
            deps.as_mut(),
            mock_env(),
            mock_info("lp", &[]),
            HandleMsg::ProposeSubscription {
                subscription: Addr::unchecked("sub_1"),
            },
        )
        .unwrap();

        // assert that the sub is store in pending review
        let res = query(deps.as_ref(), mock_env(), QueryMsg::GetSubs {}).unwrap();
        let subs: Subs = from_binary(&res).unwrap();
        assert_eq!(1, subs.pending_review.len());
    }

    #[test]
    fn accept_subscription() {
        let mut deps = mock_dependencies(&[]);

        config(&mut deps.storage)
            .save(&State {
                status: Status::Active,
                gp: Addr::unchecked("gp"),
                admin: Addr::unchecked("marketpalace"),
                qualified_tags: vec![],
                asset_denom: String::from("raise_1"),
                capital_denom: String::from("stable_coin"),
                target: 5_000_000,
                min_commitment: Some(10_000),
                max_commitment: Some(100_000),
                pending_review_subs: vec![Addr::unchecked("sub_1")].into_iter().collect(),
                accepted_subs: HashSet::new(),
                issued_calls: HashSet::new(),
                closed_calls: HashSet::new(),
            })
            .unwrap();

        // accept pending sub as gp
        let res = execute(
            deps.as_mut(),
            mock_env(),
            mock_info("gp", &[]),
            HandleMsg::AcceptSubscriptions {
                subscriptions: vec![(Addr::unchecked("sub_1"), 20_000 as u64)]
                    .into_iter()
                    .collect(),
            },
        )
        .unwrap();
        assert_eq!(1, res.messages.len());

        // assert that the sub has moved from pending review to accepted
        let res = query(deps.as_ref(), mock_env(), QueryMsg::GetSubs {}).unwrap();
        let subs: Subs = from_binary(&res).unwrap();
        assert_eq!(0, subs.pending_review.len());
        assert_eq!(1, subs.accepted.len());
    }

    #[test]
    fn issue_calls() {
        let mut deps =
            wasm_smart_mock_dependencies(&vec![], |contract_addr, _msg| match &contract_addr[..] {
                "call_1" => SystemResult::Ok(ContractResult::Ok(
                    to_binary(&CallTerms {
                        subscription: Addr::unchecked("sub_1"),
                        raise: Addr::unchecked(MOCK_CONTRACT_ADDR),
                        amount: 10_0000,
                    })
                    .unwrap(),
                )),
                _ => SystemResult::Err(SystemError::UnsupportedRequest {
                    kind: String::from("not mocked"),
                }),
            });

        config(&mut deps.storage)
            .save(&State {
                status: Status::Active,
                gp: Addr::unchecked("gp"),
                admin: Addr::unchecked("marketpalace"),
                qualified_tags: vec![],
                asset_denom: String::from("fund_coin"),
                capital_denom: String::from("stable_coin"),
                target: 5_000_000,
                min_commitment: Some(10_000),
                max_commitment: Some(100_000),
                pending_review_subs: HashSet::new(),
                accepted_subs: HashSet::new(),
                issued_calls: HashSet::new(),
                closed_calls: HashSet::new(),
            })
            .unwrap();

        // issue calls
        let res = execute(
            deps.as_mut(),
            mock_env(),
            mock_info("gp", &[]),
            HandleMsg::IssueCalls {
                calls: vec![(Addr::unchecked("call_1"))].into_iter().collect(),
            },
        )
        .unwrap();
        assert_eq!(2, res.messages.len());

        // assert that the issued call is stored
        let res = query(deps.as_ref(), mock_env(), QueryMsg::GetCalls {}).unwrap();
        let calls: Calls = from_binary(&res).unwrap();
        assert_eq!(1, calls.issued.len());
    }

    #[test]
    fn close_calls() {
        let mut deps = wasm_smart_mock_dependencies(&vec![], |contract_addr, _msg| match &contract_addr[..] {
            "call_1" => SystemResult::Ok(ContractResult::Ok(
                to_binary(&CallTerms {
                    subscription: Addr::unchecked("sub_1"),
                    raise: Addr::unchecked(MOCK_CONTRACT_ADDR),
                    amount: 10_0000,
                })
                .unwrap(),
            )),
            _ => SystemResult::Err(SystemError::UnsupportedRequest {
                kind: String::from("not mocked"),
            }),
        });

        config(&mut deps.storage)
            .save(&State {
                status: Status::Active,
                gp: Addr::unchecked("gp"),
                admin: Addr::unchecked("marketpalace"),
                qualified_tags: vec![],
                asset_denom: String::from("fund_coin"),
                capital_denom: String::from("stable_coin"),
                target: 5_000_000,
                min_commitment: Some(10_000),
                max_commitment: Some(100_000),
                pending_review_subs: HashSet::new(),
                accepted_subs: HashSet::new(),
                issued_calls: HashSet::new(),
                closed_calls: HashSet::new(),
            })
            .unwrap();

        // close call
        let res = execute(
            deps.as_mut(),
            mock_env(),
            mock_info("gp", &[]),
            HandleMsg::CloseCalls {
                calls: vec![(Addr::unchecked("call_1"))].into_iter().collect(),
            },
        )
        .unwrap();
        assert_eq!(1, res.messages.len());

        // assert that the closed call is stored
        let res = query(deps.as_ref(), mock_env(), QueryMsg::GetCalls {}).unwrap();
        let calls: Calls = from_binary(&res).unwrap();
        assert_eq!(0, calls.issued.len());
        assert_eq!(1, calls.closed.len());
    }

    #[test]
    fn issue_distributions() {
        let mut deps = mock_dependencies(&[]);

        config(&mut deps.storage)
            .save(&State {
                status: Status::Active,
                gp: Addr::unchecked("gp"),
                admin: Addr::unchecked("marketpalace"),
                qualified_tags: vec![],
                asset_denom: String::from("fund_coin"),
                capital_denom: String::from("stable_coin"),
                target: 5_000_000,
                min_commitment: Some(10_000),
                max_commitment: Some(100_000),
                pending_review_subs: HashSet::new(),
                accepted_subs: HashSet::new(),
                issued_calls: HashSet::new(),
                closed_calls: HashSet::new(),
            })
            .unwrap();

        let res = execute(
            deps.as_mut(),
            mock_env(),
            mock_info("gp", &[coin(10_000, "stable_coin")]),
            HandleMsg::IssueDistributions {
                distributions: vec![(Addr::unchecked("sub_1"), 10_000 as u64)]
                    .into_iter()
                    .collect(),
            },
        )
        .unwrap();
        assert_eq!(1, res.messages.len());
    }

    #[test]
    fn redeem_capital() {
        let mut deps = mock_dependencies(&[]);

        config(&mut deps.storage)
            .save(&State {
                status: Status::Active,
                gp: Addr::unchecked("gp"),
                admin: Addr::unchecked("marketpalace"),
                qualified_tags: vec![],
                asset_denom: String::from("fund_coin"),
                capital_denom: String::from("stable_coin"),
                target: 5_000_000,
                min_commitment: Some(10_000),
                max_commitment: Some(100_000),
                pending_review_subs: HashSet::new(),
                accepted_subs: HashSet::new(),
                issued_calls: HashSet::new(),
                closed_calls: HashSet::new(),
            })
            .unwrap();

        let res = execute(
            deps.as_mut(),
            mock_env(),
            mock_info("gp", &[]),
            HandleMsg::RedeemCapital {
                to: Addr::unchecked("omni"),
                amount: 10_000,
                memo: None,
            },
        )
        .unwrap();
        assert_eq!(1, res.messages.len());
    }
}

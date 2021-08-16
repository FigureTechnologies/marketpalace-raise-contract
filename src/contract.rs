use cosmwasm_std::{
    coin, coins, entry_point, to_binary, wasm_execute, wasm_instantiate, Addr, Attribute, BankMsg,
    Binary, ContractResult, CosmosMsg, Deps, DepsMut, Env, Event, MessageInfo, Reply, ReplyOn,
    Response, StdError, StdResult, SubMsg,
};
use provwasm_std::{
    activate_marker, create_marker, finalize_marker, grant_marker_access, MarkerAccess, MarkerType,
    ProvenanceMsg, ProvenanceQuerier,
};
use std::collections::HashMap;
use std::collections::HashSet;

use crate::error::ContractError;
use crate::msg::{
    AcceptSubscription, Call, Calls, HandleMsg, InstantiateMsg, QueryMsg, Redemption, Subs, Terms,
};
use crate::state::{config, config_read, State, Status};
use crate::sub::{SubCapitalCall, SubExecuteMsg, SubInstantiateMsg};

fn contract_error(err: &str) -> ContractError {
    ContractError::Std(StdError::generic_err(err))
}

// Note, you can use StdResult in some functions where you do not
// make use of the custom errors
#[entry_point]
pub fn instantiate(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    msg: InstantiateMsg,
) -> Result<Response<ProvenanceMsg>, ContractError> {
    let state = State {
        status: Status::Active,
        subscription_code_id: msg.subscription_code_id,
        gp: info.sender,
        admin: msg.admin,
        acceptable_accreditations: msg.acceptable_accreditations,
        other_required_tags: msg.other_required_tags,
        asset_denom: format!("{}.investment", env.contract.address),
        capital_denom: msg.capital_denom,
        target: msg.target,
        min_commitment: msg.min_commitment,
        max_commitment: msg.max_commitment,
        pending_review_subs: HashSet::new(),
        accepted_subs: HashSet::new(),
        issued_calls: HashSet::new(),
    };
    config(deps.storage).save(&state)?;

    let create = create_marker(
        msg.target as u128,
        state.asset_denom.clone(),
        MarkerType::Coin,
    )?;
    let grant = grant_marker_access(
        state.asset_denom.clone(),
        env.contract.address,
        vec![
            MarkerAccess::Admin,
            MarkerAccess::Mint,
            MarkerAccess::Burn,
            MarkerAccess::Withdraw,
        ],
    )?;
    let finalize = finalize_marker(state.asset_denom.clone())?;
    let activate = activate_marker(state.asset_denom)?;

    Ok(Response {
        submessages: vec![],
        messages: vec![create, grant, finalize, activate],
        attributes: vec![],
        data: Option::None,
    })
}

#[entry_point]
pub fn reply(deps: DepsMut, _env: Env, msg: Reply) -> Result<Response, ContractError> {
    // look for a contract address from instantiating subscription contract
    if let ContractResult::Ok(response) = msg.result {
        if let Some(contract_address) = contract_address(&response.events) {
            config(deps.storage).update(|mut state| -> Result<_, ContractError> {
                state.pending_review_subs.insert(contract_address);
                Ok(state)
            })?;
        } else {
            return Err(contract_error("no contract address found"));
        }
    } else {
        return Err(contract_error("subscription contract instantiation failed"));
    }

    Ok(Response::default())
}

fn contract_address(events: &[Event]) -> Option<Addr> {
    events.first().and_then(|event| {
        event
            .attributes
            .iter()
            .find(|attr| attr.key == "contract_address")
            .map(|attr| Addr::unchecked(attr.value.clone()))
    })
}

// And declare a custom Error variant for the ones where you will want to make use of it
#[entry_point]
pub fn execute(
    deps: DepsMut,
    _env: Env,
    info: MessageInfo,
    msg: HandleMsg,
) -> Result<Response<ProvenanceMsg>, ContractError> {
    match msg {
        HandleMsg::Recover { gp } => try_recover(deps, info, gp),
        HandleMsg::ProposeSubscription {
            min_commitment,
            max_commitment,
            min_days_of_notice,
        } => try_propose_subscription(
            deps,
            info,
            min_commitment,
            max_commitment,
            min_days_of_notice,
        ),
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
    info: MessageInfo,
    min_commitment: u64,
    max_commitment: u64,
    min_days_of_notice: Option<u16>,
) -> Result<Response<ProvenanceMsg>, ContractError> {
    let state = config_read(deps.storage).load()?;

    if state.status != Status::Active {
        return Err(contract_error("contract is not active"));
    }

    if let Some(min) = state.min_commitment {
        if max_commitment < min {
            return Err(contract_error(
                "subscription max commitment is below raise minumum commitment",
            ));
        }
    }

    if let Some(max) = state.max_commitment {
        if min_commitment > max {
            return Err(contract_error(
                "subscription min commitment exceeds raise maximum commitment",
            ));
        }
    }

    let attributes: HashSet<String> = ProvenanceQuerier::new(&deps.querier)
        .get_attributes(info.sender.clone(), None as Option<String>)?
        .attributes
        .into_iter()
        .map(|attribute| attribute.name)
        .collect();

    if !state.acceptable_accreditations.is_empty()
        && attributes
            .intersection(&state.acceptable_accreditations)
            .count()
            == 0
    {
        return Err(contract_error(&format!(
            "subscription owner must have one of acceptable accreditations: {:?}",
            state.acceptable_accreditations
        )));
    }

    if attributes.intersection(&state.other_required_tags).count()
        != state.other_required_tags.len()
    {
        return Err(contract_error(&format!(
            "subscription owner must have all other required tags: {:?}",
            state.other_required_tags
        )));
    }

    Ok(Response {
        submessages: vec![SubMsg {
            id: 1,
            msg: CosmosMsg::Wasm(
                wasm_instantiate(
                    state.subscription_code_id,
                    &SubInstantiateMsg {
                        lp: info.sender,
                        admin: state.admin,
                        capital_denom: state.capital_denom,
                        min_commitment,
                        max_commitment,
                        min_days_of_notice,
                    },
                    vec![],
                    String::from("establish subscription"),
                )
                .unwrap(),
            ),
            gas_limit: None,
            reply_on: ReplyOn::Always,
        }],
        messages: vec![],
        attributes: vec![],
        data: Option::None,
    })
}

pub fn try_accept_subscriptions(
    deps: DepsMut,
    info: MessageInfo,
    subscriptions: HashSet<AcceptSubscription>,
) -> Result<Response<ProvenanceMsg>, ContractError> {
    let state = config_read(deps.storage).load()?;

    if info.sender != state.gp {
        return Err(contract_error("only gp can accept subscriptions"));
    }

    config(deps.storage).update(|mut state| -> Result<_, ContractError> {
        subscriptions.iter().for_each(|accept| {
            state.pending_review_subs.remove(&accept.subscription);
            state.accepted_subs.insert(accept.subscription.clone());
        });

        Ok(state)
    })?;

    Ok(Response {
        submessages: vec![],
        messages: subscriptions
            .into_iter()
            .map(|accept| {
                CosmosMsg::Wasm(
                    wasm_execute(
                        accept.subscription,
                        &SubExecuteMsg::Accept {},
                        coins(accept.commitment as u128, state.asset_denom.clone()),
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
    calls: HashSet<Call>,
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
        .map(|call| {
            CosmosMsg::Wasm(
                wasm_execute(
                    call.subscription,
                    &SubExecuteMsg::IssueCapitalCall {
                        capital_call: SubCapitalCall {
                            amount: call.amount,
                            days_of_notice: call.days_of_notice,
                        },
                    },
                    vec![],
                )
                .unwrap(),
            )
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

    let calls_to_close: Vec<Call> = state
        .issued_calls
        .into_iter()
        .filter(|call| calls.contains(&call.subscription))
        .collect();

    config(deps.storage).update(|mut state| -> Result<_, ContractError> {
        calls_to_close.iter().for_each(|call| {
            state.issued_calls.remove(call);
        });

        Ok(state)
    })?;

    let close_messages = calls_to_close
        .into_iter()
        .map(|call| {
            CosmosMsg::Wasm(
                wasm_execute(
                    call.subscription.clone(),
                    &SubExecuteMsg::CloseCapitalCall {},
                    coins(
                        call.amount as u128,
                        format!("{}.commitment", call.subscription),
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
            acceptable_accreditations: state.acceptable_accreditations,
            other_required_tags: state.other_required_tags,
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
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use cosmwasm_std::testing::{mock_env, mock_info};
    use cosmwasm_std::{from_binary, Addr};
    use provwasm_mocks::mock_dependencies;

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
                subscription_code_id: 0,
                admin: Addr::unchecked("marketpalace"),
                acceptable_accreditations: HashSet::new(),
                other_required_tags: HashSet::new(),
                capital_denom: String::from("stable_coin"),
                target: 5_000_000,
                min_commitment: Some(10_000),
                max_commitment: Some(100_000),
            },
        )
        .unwrap();
        assert_eq!(4, res.messages.len());

        // verify raise is in active status
        let res = query(deps.as_ref(), mock_env(), QueryMsg::GetStatus {}).unwrap();
        let status: Status = from_binary(&res).unwrap();
        assert_eq!(Status::Active, status);

        // verify that terms of raise are correct
        let res = query(deps.as_ref(), mock_env(), QueryMsg::GetTerms {}).unwrap();
        let terms: Terms = from_binary(&res).unwrap();
        assert_eq!(0, terms.acceptable_accreditations.len());
        assert_eq!(0, terms.other_required_tags.len());
        assert_eq!("cosmos2contract.investment", terms.asset_denom);
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
                subscription_code_id: 0,
                gp: Addr::unchecked("gp"),
                admin: Addr::unchecked("marketpalace"),
                acceptable_accreditations: HashSet::new(),
                other_required_tags: HashSet::new(),
                asset_denom: String::from("fund_coin"),
                capital_denom: String::from("stable_coin"),
                target: 5_000_000,
                min_commitment: Some(10_000),
                max_commitment: Some(100_000),
                pending_review_subs: HashSet::new(),
                accepted_subs: HashSet::new(),
                issued_calls: HashSet::new(),
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
                subscription_code_id: 0,
                gp: Addr::unchecked("gp"),
                admin: Addr::unchecked("marketpalace"),
                acceptable_accreditations: HashSet::new(),
                other_required_tags: HashSet::new(),
                asset_denom: String::from("fund_coin"),
                capital_denom: String::from("stable_coin"),
                target: 5_000_000,
                min_commitment: Some(10_000),
                max_commitment: Some(100_000),
                pending_review_subs: HashSet::new(),
                accepted_subs: HashSet::new(),
                issued_calls: HashSet::new(),
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
        let mut deps = mock_dependencies(&vec![]);

        config(&mut deps.storage)
            .save(&State {
                status: Status::Active,
                subscription_code_id: 0,
                gp: Addr::unchecked("gp"),
                admin: Addr::unchecked("marketpalace"),
                acceptable_accreditations: HashSet::new(),
                other_required_tags: HashSet::new(),
                asset_denom: String::from("fund_coin"),
                capital_denom: String::from("stable_coin"),
                target: 5_000_000,
                min_commitment: Some(10_000),
                max_commitment: Some(100_000),
                pending_review_subs: HashSet::new(),
                accepted_subs: HashSet::new(),
                issued_calls: HashSet::new(),
            })
            .unwrap();

        // propose a sub as lp
        let res = execute(
            deps.as_mut(),
            mock_env(),
            mock_info("lp", &[]),
            HandleMsg::ProposeSubscription {
                min_commitment: 10_000,
                max_commitment: 100_000,
                min_days_of_notice: None,
            },
        )
        .unwrap();
        assert_eq!(1, res.submessages.len());
    }

    #[test]
    fn accept_subscription() {
        let mut deps = mock_dependencies(&[]);

        config(&mut deps.storage)
            .save(&State {
                status: Status::Active,
                subscription_code_id: 0,
                gp: Addr::unchecked("gp"),
                admin: Addr::unchecked("marketpalace"),
                acceptable_accreditations: HashSet::new(),
                other_required_tags: HashSet::new(),
                asset_denom: String::from("raise_1"),
                capital_denom: String::from("stable_coin"),
                target: 5_000_000,
                min_commitment: Some(10_000),
                max_commitment: Some(100_000),
                pending_review_subs: vec![Addr::unchecked("sub_1")].into_iter().collect(),
                accepted_subs: HashSet::new(),
                issued_calls: HashSet::new(),
            })
            .unwrap();

        // accept pending sub as gp
        let res = execute(
            deps.as_mut(),
            mock_env(),
            mock_info("gp", &[]),
            HandleMsg::AcceptSubscriptions {
                subscriptions: vec![AcceptSubscription {
                    subscription: Addr::unchecked("sub_1"),
                    commitment: 20_000,
                }]
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
        let mut deps = mock_dependencies(&vec![]);

        config(&mut deps.storage)
            .save(&State {
                status: Status::Active,
                subscription_code_id: 0,
                gp: Addr::unchecked("gp"),
                admin: Addr::unchecked("marketpalace"),
                acceptable_accreditations: HashSet::new(),
                other_required_tags: HashSet::new(),
                asset_denom: String::from("fund_coin"),
                capital_denom: String::from("stable_coin"),
                target: 5_000_000,
                min_commitment: Some(10_000),
                max_commitment: Some(100_000),
                pending_review_subs: HashSet::new(),
                accepted_subs: HashSet::new(),
                issued_calls: HashSet::new(),
            })
            .unwrap();

        // issue calls
        let res = execute(
            deps.as_mut(),
            mock_env(),
            mock_info("gp", &[]),
            HandleMsg::IssueCalls {
                calls: vec![Call {
                    subscription: Addr::unchecked("sub_1"),
                    amount: 10_000,
                    days_of_notice: None,
                }]
                .into_iter()
                .collect(),
            },
        )
        .unwrap();
        assert_eq!(1, res.messages.len());

        // assert that the issued call is stored
        let res = query(deps.as_ref(), mock_env(), QueryMsg::GetCalls {}).unwrap();
        let calls: Calls = from_binary(&res).unwrap();
        assert_eq!(1, calls.issued.len());
    }

    #[test]
    fn close_calls() {
        let mut deps = mock_dependencies(&vec![]);

        config(&mut deps.storage)
            .save(&State {
                status: Status::Active,
                subscription_code_id: 0,
                gp: Addr::unchecked("gp"),
                admin: Addr::unchecked("marketpalace"),
                acceptable_accreditations: HashSet::new(),
                other_required_tags: HashSet::new(),
                asset_denom: String::from("fund_coin"),
                capital_denom: String::from("stable_coin"),
                target: 5_000_000,
                min_commitment: Some(10_000),
                max_commitment: Some(100_000),
                pending_review_subs: HashSet::new(),
                accepted_subs: HashSet::new(),
                issued_calls: vec![Call {
                    subscription: Addr::unchecked("sub_1"),
                    amount: 10_000,
                    days_of_notice: None,
                }]
                .into_iter()
                .collect(),
            })
            .unwrap();

        // close call
        let res = execute(
            deps.as_mut(),
            mock_env(),
            mock_info("gp", &[]),
            HandleMsg::CloseCalls {
                calls: vec![(Addr::unchecked("sub_1"))].into_iter().collect(),
            },
        )
        .unwrap();
        assert_eq!(1, res.messages.len());

        // assert that the closed call is stored
        let res = query(deps.as_ref(), mock_env(), QueryMsg::GetCalls {}).unwrap();
        let calls: Calls = from_binary(&res).unwrap();
        assert_eq!(0, calls.issued.len());
    }

    #[test]
    fn issue_distributions() {
        let mut deps = mock_dependencies(&[]);

        config(&mut deps.storage)
            .save(&State {
                status: Status::Active,
                subscription_code_id: 0,
                gp: Addr::unchecked("gp"),
                admin: Addr::unchecked("marketpalace"),
                acceptable_accreditations: HashSet::new(),
                other_required_tags: HashSet::new(),
                asset_denom: String::from("fund_coin"),
                capital_denom: String::from("stable_coin"),
                target: 5_000_000,
                min_commitment: Some(10_000),
                max_commitment: Some(100_000),
                pending_review_subs: HashSet::new(),
                accepted_subs: HashSet::new(),
                issued_calls: HashSet::new(),
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
                subscription_code_id: 0,
                gp: Addr::unchecked("gp"),
                admin: Addr::unchecked("marketpalace"),
                acceptable_accreditations: HashSet::new(),
                other_required_tags: HashSet::new(),
                asset_denom: String::from("fund_coin"),
                capital_denom: String::from("stable_coin"),
                target: 5_000_000,
                min_commitment: Some(10_000),
                max_commitment: Some(100_000),
                pending_review_subs: HashSet::new(),
                accepted_subs: HashSet::new(),
                issued_calls: HashSet::new(),
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

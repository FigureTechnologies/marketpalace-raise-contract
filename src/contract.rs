use crate::error::contract_error;
use crate::recover::try_recover;
use crate::subscribe::try_accept_subscriptions;
use crate::subscribe::try_propose_subscription;
use cosmwasm_std::{
    coin, coins, entry_point, wasm_execute, Addr, Attribute, BankMsg, ContractResult, CosmosMsg,
    DepsMut, Env, Event, MessageInfo, Reply, Response,
};
use provwasm_std::{withdraw_coins, ProvenanceMsg};
use std::collections::HashSet;

use crate::error::ContractError;
use crate::msg::{CallClosure, CallIssuance, Distribution, HandleMsg, Redemption};
use crate::state::{config, config_read, Withdrawal};
use crate::sub_msg::{SubCapitalCallIssuance, SubExecuteMsg, SubQueryMsg, SubTransactions};

pub type ContractResponse = Result<Response<ProvenanceMsg>, ContractError>;

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
            return contract_error("no contract address found");
        }
    } else {
        return contract_error("subscription contract instantiation failed");
    }

    Ok(Response::default())
}

fn contract_address(events: &[Event]) -> Option<Addr> {
    events.first().and_then(|event| {
        event
            .attributes
            .iter()
            .find(|attr| attr.key == "_contract_address")
            .map(|attr| Addr::unchecked(attr.value.clone()))
    })
}

#[entry_point]
pub fn execute(deps: DepsMut, env: Env, info: MessageInfo, msg: HandleMsg) -> ContractResponse {
    match msg {
        HandleMsg::Recover { gp } => try_recover(deps, info, gp),
        HandleMsg::ProposeSubscription {
            min_commitment,
            max_commitment,
            min_days_of_notice,
        } => try_propose_subscription(
            deps,
            env,
            info,
            min_commitment,
            max_commitment,
            min_days_of_notice,
        ),
        HandleMsg::AcceptSubscriptions { subscriptions } => {
            try_accept_subscriptions(deps, env, info, subscriptions)
        }
        HandleMsg::IssueCapitalCalls { calls } => try_issue_calls(deps, info, calls),
        HandleMsg::CloseCapitalCalls {
            calls,
            is_retroactive,
        } => try_close_calls(deps, env, info, calls, is_retroactive),
        HandleMsg::IssueRedemptions {
            redemptions,
            is_retroactive,
        } => try_issue_redemptions(deps, info, redemptions, is_retroactive),
        HandleMsg::IssueDistributions {
            distributions,
            is_retroactive,
        } => try_issue_distributions(deps, info, distributions, is_retroactive),
        HandleMsg::IssueWithdrawal { to, amount, memo } => {
            try_issue_withdrawal(deps, info, env, to, amount, memo)
        }
    }
}

pub fn try_issue_calls(
    deps: DepsMut,
    info: MessageInfo,
    calls: HashSet<CallIssuance>,
) -> ContractResponse {
    let state = config_read(deps.storage).load()?;

    if info.sender != state.gp {
        return contract_error("only gp can issue calls");
    }

    let calls: Vec<CosmosMsg<ProvenanceMsg>> = calls
        .into_iter()
        .map(|call| {
            CosmosMsg::Wasm(
                wasm_execute(
                    call.subscription,
                    &SubExecuteMsg::IssueCapitalCall {
                        capital_call: SubCapitalCallIssuance {
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

    Ok(Response::new().add_messages(calls))
}

pub fn try_close_calls(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    calls: HashSet<CallClosure>,
    is_retroactive: bool,
) -> ContractResponse {
    let state = config_read(deps.storage).load()?;

    if info.sender != state.gp {
        return contract_error("only gp can close calls");
    }

    let close_messages: Vec<CosmosMsg<ProvenanceMsg>> = calls
        .into_iter()
        .flat_map(|call| {
            let transactions: SubTransactions = deps
                .querier
                .query_wasm_smart(call.subscription.clone(), &SubQueryMsg::GetTransactions {})
                .unwrap();

            let active_call_amount = transactions.capital_calls.active.unwrap().amount;

            vec![
                withdraw_coins(
                    state.investment_denom.clone(),
                    state.capital_to_shares(active_call_amount) as u128,
                    state.investment_denom.clone(),
                    env.contract.address.clone(),
                )
                .unwrap(),
                CosmosMsg::Wasm(
                    wasm_execute(
                        call.subscription,
                        &SubExecuteMsg::CloseCapitalCall { is_retroactive },
                        coins(
                            state.capital_to_shares(active_call_amount) as u128,
                            state.investment_denom.clone(),
                        ),
                    )
                    .unwrap(),
                ),
            ]
        })
        .collect();

    Ok(Response::new().add_messages(close_messages))
}

pub fn try_issue_redemptions(
    deps: DepsMut,
    info: MessageInfo,
    redemptions: HashSet<Redemption>,
    is_retroactive: bool,
) -> ContractResponse {
    let state = config_read(deps.storage).load()?;

    if info.sender != state.gp {
        return contract_error("only gp can issue redemptions");
    }

    let redemptions: Vec<CosmosMsg<ProvenanceMsg>> = redemptions
        .into_iter()
        .map(|redemption| {
            CosmosMsg::Wasm(
                wasm_execute(
                    redemption.subscription,
                    &SubExecuteMsg::IssueRedemption {
                        redemption: redemption.asset,
                        payment: redemption.capital,
                        is_retroactive,
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

    Ok(Response::new().add_messages(redemptions))
}

pub fn try_issue_distributions(
    deps: DepsMut,
    info: MessageInfo,
    distributions: HashSet<Distribution>,
    is_retroactive: bool,
) -> ContractResponse {
    let state = config_read(deps.storage).load()?;

    if info.sender != state.gp {
        return contract_error("only gp can issue distributions");
    }

    let distributions: Vec<CosmosMsg<ProvenanceMsg>> = distributions
        .into_iter()
        .map(|distribution| {
            CosmosMsg::Wasm(
                wasm_execute(
                    distribution.subscription,
                    &SubExecuteMsg::IssueDistribution {
                        payment: distribution.amount,
                        is_retroactive,
                    },
                    vec![coin(
                        distribution.amount as u128,
                        state.capital_denom.clone(),
                    )],
                )
                .unwrap(),
            )
        })
        .collect();

    Ok(Response::new().add_messages(distributions))
}

pub fn try_issue_withdrawal(
    deps: DepsMut,
    info: MessageInfo,
    env: Env,
    to: Addr,
    amount: u64,
    memo: Option<String>,
) -> ContractResponse {
    let state = config_read(deps.storage).load()?;

    if info.sender != state.gp {
        return contract_error("only gp can redeem capital");
    }

    config(deps.storage).update(|mut state| -> Result<_, ContractError> {
        state.sequence += 1;
        state.issued_withdrawals.insert(Withdrawal {
            sequence: state.sequence,
            to: to.clone(),
            amount,
        });
        Ok(state)
    })?;

    let state = config_read(deps.storage).load()?;

    let send = BankMsg::Send {
        to_address: to.to_string(),
        amount: vec![coin(amount as u128, state.capital_denom)],
    };

    let sequence_attribute = Attribute {
        key: format!("{}.withdrawal.sequence", env.contract.address),
        value: format!("{}", state.sequence),
    };

    let attributes = match memo {
        Some(memo) => {
            vec![
                Attribute {
                    key: String::from("memo"),
                    value: memo,
                },
                sequence_attribute,
            ]
        }
        None => vec![sequence_attribute],
    };

    Ok(Response::new().add_message(send).add_attributes(attributes))
}

#[cfg(test)]
pub mod tests {
    use super::*;
    use crate::mock::{wasm_smart_mock_dependencies, MockContractQuerier};
    use crate::state::State;
    use crate::sub_msg::SubTerms;
    use crate::sub_msg::{SubCapitalCall, SubCapitalCalls};
    use cosmwasm_std::testing::{mock_env, mock_info, MockApi, MockStorage};
    use cosmwasm_std::to_binary;
    use cosmwasm_std::{Addr, MemoryStorage, OwnedDeps, SystemResult};
    use provwasm_mocks::{mock_dependencies, ProvenanceMockQuerier};

    pub fn default_deps(
        update_state: Option<fn(&mut State)>,
    ) -> OwnedDeps<MockStorage, MockApi, ProvenanceMockQuerier> {
        let mut deps = mock_dependencies(&[]);

        let mut state = State::test_default();
        if let Some(update) = update_state {
            update(&mut state);
        }
        config(&mut deps.storage).save(&state).unwrap();

        deps
    }

    pub fn mock_sub_terms() -> OwnedDeps<MemoryStorage, MockApi, MockContractQuerier> {
        wasm_smart_mock_dependencies(&vec![], |_, _| {
            SystemResult::Ok(ContractResult::Ok(
                to_binary(&SubTerms {
                    lp: Addr::unchecked("lp"),
                    raise: Addr::unchecked("raise_1"),
                    capital_denom: String::from("stable_coin"),
                    min_commitment: 10_000,
                    max_commitment: 100_000,
                })
                .unwrap(),
            ))
        })
    }

    #[test]
    fn issue_calls() {
        let mut deps = default_deps(None);

        // issue calls
        let res = execute(
            deps.as_mut(),
            mock_env(),
            mock_info("gp", &[]),
            HandleMsg::IssueCapitalCalls {
                calls: vec![CallIssuance {
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
    }

    #[test]
    fn close_calls() {
        let mut deps = wasm_smart_mock_dependencies(&vec![], |_, _| {
            SystemResult::Ok(ContractResult::Ok(
                to_binary(&SubTransactions {
                    capital_calls: SubCapitalCalls {
                        active: Some(SubCapitalCall {
                            sequence: 1,
                            amount: 10_000,
                            days_of_notice: None,
                        }),
                        closed: HashSet::new(),
                        cancelled: HashSet::new(),
                    },
                    redemptions: HashSet::new(),
                    distributions: HashSet::new(),
                    withdrawals: HashSet::new(),
                })
                .unwrap(),
            ))
        });

        config(&mut deps.storage)
            .save(&State::test_default())
            .unwrap();

        // close call
        let res = execute(
            deps.as_mut(),
            mock_env(),
            mock_info("gp", &[]),
            HandleMsg::CloseCapitalCalls {
                calls: vec![CallClosure {
                    subscription: Addr::unchecked("sub_1"),
                }]
                .into_iter()
                .collect(),
                is_retroactive: false,
            },
        )
        .unwrap();
        assert_eq!(2, res.messages.len());
    }

    #[test]
    fn issue_distributions() {
        let mut deps = default_deps(None);

        let res = execute(
            deps.as_mut(),
            mock_env(),
            mock_info("gp", &[coin(10_000, "stable_coin")]),
            HandleMsg::IssueDistributions {
                distributions: vec![Distribution {
                    subscription: Addr::unchecked("sub_1"),
                    amount: 10_000,
                }]
                .into_iter()
                .collect(),
                is_retroactive: false,
            },
        )
        .unwrap();
        assert_eq!(1, res.messages.len());
    }

    #[test]
    fn redeem_capital() {
        let mut deps = default_deps(None);

        let res = execute(
            deps.as_mut(),
            mock_env(),
            mock_info("gp", &[]),
            HandleMsg::IssueWithdrawal {
                to: Addr::unchecked("omni"),
                amount: 10_000,
                memo: None,
            },
        )
        .unwrap();
        assert_eq!(1, res.messages.len());
    }
}

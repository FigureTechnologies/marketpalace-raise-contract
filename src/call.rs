use crate::contract::ContractResponse;
use crate::error::contract_error;
use crate::msg::CallClosure;
use crate::msg::CallIssuance;
use crate::state::config_read;
use crate::sub_msg::SubCapitalCallIssuance;
use crate::sub_msg::SubExecuteMsg;
use crate::sub_msg::SubQueryMsg;
use crate::sub_msg::SubTransactions;
use cosmwasm_std::coins;
use cosmwasm_std::wasm_execute;
use cosmwasm_std::CosmosMsg;
use cosmwasm_std::DepsMut;
use cosmwasm_std::Env;
use cosmwasm_std::MessageInfo;
use cosmwasm_std::Response;
use provwasm_std::withdraw_coins;
use provwasm_std::ProvenanceMsg;
use std::collections::HashSet;

pub fn try_issue_calls(
    deps: DepsMut,
    info: MessageInfo,
    calls: HashSet<CallIssuance>,
) -> ContractResponse {
    let state = config_read(deps.storage).load()?;

    if info.sender != state.gp {
        return contract_error("only gp can issue calls");
    }

    Ok(Response::new().add_messages(calls.into_iter().map(|call| {
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
        .unwrap()
    })))
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

#[cfg(test)]
mod tests {
    use crate::contract::execute;
    use crate::contract::tests::default_deps;
    use crate::mock::marker_msg;
    use crate::mock::msg_at_index;
    use crate::mock::wasm_msg;
    use crate::mock::wasm_smart_mock_dependencies;
    use crate::msg::CallClosure;
    use crate::msg::CallIssuance;
    use crate::msg::HandleMsg;
    use crate::state::config;
    use crate::state::State;
    use crate::sub_msg::SubCapitalCall;
    use crate::sub_msg::SubCapitalCalls;
    use crate::sub_msg::SubTransactions;
    use cosmwasm_std::testing::mock_env;
    use cosmwasm_std::testing::mock_info;
    use cosmwasm_std::to_binary;
    use cosmwasm_std::Addr;
    use cosmwasm_std::ContractResult;
    use cosmwasm_std::SystemResult;
    use cosmwasm_std::WasmMsg;
    use provwasm_std::MarkerMsgParams;
    use std::collections::HashSet;

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

        // verify wasm execute message is sent
        assert_eq!(1, res.messages.len());
        assert!(matches!(
            wasm_msg(msg_at_index(&res, 0)),
            WasmMsg::Execute { .. }
        ))
    }

    #[test]

    fn issue_calls_bad_actor() {
        let mut deps = default_deps(None);

        // issue calls
        let res = execute(
            deps.as_mut(),
            mock_env(),
            mock_info("bad_actor", &[]),
            HandleMsg::IssueCapitalCalls {
                calls: vec![CallIssuance {
                    subscription: Addr::unchecked("sub_1"),
                    amount: 10_000,
                    days_of_notice: None,
                }]
                .into_iter()
                .collect(),
            },
        );
        assert!(res.is_err());
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

        // verify that withdraw and execute messages are sent
        assert_eq!(2, res.messages.len());
        assert!(matches!(
            marker_msg(msg_at_index(&res, 0)),
            MarkerMsgParams::WithdrawCoins { .. }
        ));
        assert!(matches!(
            wasm_msg(msg_at_index(&res, 1)),
            WasmMsg::Execute { .. }
        ));
    }

    #[test]
    fn close_calls_bad_actor() {
        let mut deps = default_deps(None);

        // close call
        let res = execute(
            deps.as_mut(),
            mock_env(),
            mock_info("bad_actor", &[]),
            HandleMsg::CloseCapitalCalls {
                calls: vec![CallClosure {
                    subscription: Addr::unchecked("sub_1"),
                }]
                .into_iter()
                .collect(),
                is_retroactive: false,
            },
        );
        assert!(res.is_err());
    }
}

use crate::contract::ContractResponse;
use crate::error::contract_error;
use crate::msg::CapitalCall;
use crate::state::config_read;
use crate::state::outstanding_capital_calls;
use cosmwasm_std::coins;
use cosmwasm_std::BankMsg;
use cosmwasm_std::DepsMut;
use cosmwasm_std::Env;
use cosmwasm_std::MessageInfo;
use cosmwasm_std::Response;
use provwasm_std::burn_marker_supply;
use provwasm_std::mint_marker_supply;
use provwasm_std::withdraw_coins;
use provwasm_std::ProvenanceQuerier;
use provwasm_std::ProvenanceQuery;

pub fn try_issue_calls(
    deps: DepsMut<ProvenanceQuery>,
    env: Env,
    info: MessageInfo,
    mut calls: Vec<CapitalCall>,
) -> ContractResponse {
    let state = config_read(deps.storage).load()?;

    if info.sender != state.gp {
        return contract_error("only gp can issue calls");
    }

    if let Some(mut existing) = outstanding_capital_calls(deps.storage).may_load()? {
        calls.append(&mut existing)
    }

    outstanding_capital_calls(deps.storage).save(&calls)?;

    let investment_total = calls.iter().map(|it| it.amount).sum();
    let supply = state.capital_to_shares(investment_total);
    let mint = mint_marker_supply(supply.into(), state.investment_denom.clone())?;
    let withdraw = withdraw_coins(
        state.investment_denom.clone(),
        supply.into(),
        state.investment_denom,
        env.contract.address,
    )?;

    Ok(Response::new().add_messages(vec![mint, withdraw]))
}

pub fn try_cancel_calls(
    deps: DepsMut<ProvenanceQuery>,
    info: MessageInfo,
    calls: Vec<CapitalCall>,
) -> ContractResponse {
    let state = config_read(deps.storage).load()?;

    if info.sender != state.gp {
        return contract_error("only gp can cancel capital calls");
    }

    if let Some(mut existing) = outstanding_capital_calls(deps.storage).may_load()? {
        for call in calls {
            if let Some(index) = existing
                .iter()
                .position(|it| it.subscription == call.subscription && it.amount == call.amount)
            {
                existing.remove(index)
            } else {
                return contract_error("no capital call found");
            };
        }

        outstanding_capital_calls(deps.storage).save(&existing)?;
    } else {
        return contract_error("no outstanding capital calls to cancel");
    };

    Ok(Response::default())
}

pub fn try_claim_investment(
    deps: DepsMut<ProvenanceQuery>,
    info: MessageInfo,
    amount: u64,
) -> ContractResponse {
    let state = config_read(deps.storage).load()?;

    let mut calls = outstanding_capital_calls(deps.storage).load()?;
    let call = if let Some(index) = calls
        .iter()
        .position(|it| it.subscription == info.sender && it.amount == amount)
    {
        calls.remove(index)
    } else {
        return contract_error("no call for subscription");
    };

    match info
        .funds
        .iter()
        .find(|it| it.denom == state.commitment_denom)
    {
        Some(commitment) => {
            if commitment.amount.u128() != state.capital_to_shares(call.amount).into() {
                return contract_error("sent funds should match specified commitment");
            }
        }
        None => return contract_error("commitment required for investment"),
    };

    match info.funds.iter().find(|it| it.denom == state.capital_denom) {
        Some(capital) => {
            if capital.amount.u128() != call.amount.into() {
                return contract_error("sent funds should match specified capital");
            }
        }
        None => return contract_error("capital required for investment"),
    };

    let send_investment = BankMsg::Send {
        to_address: call.subscription.into_string(),
        amount: coins(
            state.capital_to_shares(call.amount).into(),
            state.investment_denom.clone(),
        ),
    };

    let commitment_marker = ProvenanceQuerier::new(&deps.querier)
        .get_marker_by_denom(state.commitment_denom.clone())?;
    let deposit_commitment = BankMsg::Send {
        to_address: commitment_marker.address.into_string(),
        amount: coins(
            state.capital_to_shares(call.amount).into(),
            state.commitment_denom.clone(),
        ),
    };
    let burn_commitment = burn_marker_supply(
        state.capital_to_shares(call.amount).into(),
        state.commitment_denom,
    )?;

    Ok(Response::new()
        .add_message(send_investment)
        .add_message(deposit_commitment)
        .add_message(burn_commitment))
}

#[cfg(test)]
mod tests {
    use crate::contract::execute;
    use crate::contract::tests::default_deps;
    use crate::mock::burn_args;
    use crate::mock::load_markers;
    use crate::mock::mint_args;
    use crate::mock::msg_at_index;
    use crate::mock::send_args;
    use crate::mock::withdraw_args;
    use crate::msg::CapitalCall;
    use crate::msg::HandleMsg;
    use crate::state::outstanding_capital_calls;
    use cosmwasm_std::coin;
    use cosmwasm_std::testing::mock_env;
    use cosmwasm_std::testing::mock_info;
    use cosmwasm_std::Addr;

    #[test]

    fn issue_calls() {
        let mut deps = default_deps(None);
        outstanding_capital_calls(&mut deps.storage)
            .save(&vec![CapitalCall {
                subscription: Addr::unchecked("sub_1"),
                amount: 10_000,
            }])
            .unwrap();

        // issue calls
        let res = execute(
            deps.as_mut(),
            mock_env(),
            mock_info("gp", &[]),
            HandleMsg::IssueCapitalCalls {
                calls: vec![CapitalCall {
                    subscription: Addr::unchecked("sub_2"),
                    amount: 10_000,
                }]
                .into_iter()
                .collect(),
            },
        )
        .unwrap();

        assert_eq!(2, res.messages.len());

        // verify minted coin
        let mint = mint_args(msg_at_index(&res, 0));
        assert_eq!(200, mint.amount.u128());
        assert_eq!("investment_coin", mint.denom);

        // verify withdrawn coin
        let (marker_denom, coin, recipient) = withdraw_args(msg_at_index(&res, 1));
        assert_eq!("investment_coin", marker_denom);
        assert_eq!(200, coin.amount.u128());
        assert_eq!("investment_coin", coin.denom);
        assert_eq!("cosmos2contract", recipient.clone().into_string());

        // verify capital call is saved
        assert_eq!(
            2,
            outstanding_capital_calls(&mut deps.storage)
                .load()
                .unwrap()
                .len()
        )
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
                calls: vec![CapitalCall {
                    subscription: Addr::unchecked("sub_1"),
                    amount: 10_000,
                }]
                .into_iter()
                .collect(),
            },
        );

        assert!(res.is_err());
    }

    #[test]

    fn cancel_calls() {
        let mut deps = default_deps(None);
        outstanding_capital_calls(&mut deps.storage)
            .save(&vec![CapitalCall {
                subscription: Addr::unchecked("sub_1"),
                amount: 10_000,
            }])
            .unwrap();

        // cancel calls
        let res = execute(
            deps.as_mut(),
            mock_env(),
            mock_info("gp", &[]),
            HandleMsg::CancelCapitalCalls {
                calls: vec![CapitalCall {
                    subscription: Addr::unchecked("sub_1"),
                    amount: 10_000,
                }]
                .into_iter()
                .collect(),
            },
        )
        .unwrap();

        // verify capital call is removed
        assert_eq!(
            0,
            outstanding_capital_calls(&mut deps.storage)
                .load()
                .unwrap()
                .len()
        )
    }

    #[test]

    fn cancel_calls_bad_actor() {
        let mut deps = default_deps(None);

        // issue calls
        let res = execute(
            deps.as_mut(),
            mock_env(),
            mock_info("bad_actor", &[]),
            HandleMsg::CancelCapitalCalls {
                calls: vec![].into_iter().collect(),
            },
        );

        assert!(res.is_err());
    }

    #[test]
    fn claim_investment() {
        let mut deps = default_deps(None);
        load_markers(&mut deps.querier);
        outstanding_capital_calls(&mut deps.storage)
            .save(&vec![CapitalCall {
                subscription: Addr::unchecked("sub_1"),
                amount: 10_000,
            }])
            .unwrap();

        // close call
        let res = execute(
            deps.as_mut(),
            mock_env(),
            mock_info(
                "sub_1",
                &vec![coin(100, "commitment_coin"), coin(10_000, "stable_coin")],
            ),
            HandleMsg::ClaimInvestment { amount: 10_000 },
        )
        .unwrap();

        assert_eq!(3, res.messages.len());

        // verify send investment
        let (to_address, coins) = send_args(msg_at_index(&res, 0));
        assert_eq!("sub_1", to_address);
        assert_eq!("investment_coin", coins.first().unwrap().denom);
        assert_eq!(100, coins.first().unwrap().amount.u128());

        // verify deposit commitment
        let (to_address, coins) = send_args(msg_at_index(&res, 1));
        assert_eq!("tp18vmzryrvwaeykmdtu6cfrz5sau3dhc5c73ms0u", to_address);
        assert_eq!("commitment_coin", coins.first().unwrap().denom);
        assert_eq!(100, coins.first().unwrap().amount.u128());

        // verify burn commitment
        let coin = burn_args(msg_at_index(&res, 2));
        assert_eq!("commitment_coin", coin.denom);
        assert_eq!(100, coin.amount.u128());
    }

    #[test]
    fn claim_investment_missing_commitment() {
        let mut deps = default_deps(None);
        load_markers(&mut deps.querier);
        outstanding_capital_calls(&mut deps.storage)
            .save(&vec![CapitalCall {
                subscription: Addr::unchecked("sub_1"),
                amount: 10_000,
            }])
            .unwrap();

        // close call
        let res = execute(
            deps.as_mut(),
            mock_env(),
            mock_info("sub_1", &vec![coin(10_000, "stable_coin")]),
            HandleMsg::ClaimInvestment { amount: 10_000 },
        );

        assert!(res.is_err());
    }

    #[test]
    fn claim_investment_missing_capital() {
        let mut deps = default_deps(None);
        load_markers(&mut deps.querier);
        outstanding_capital_calls(&mut deps.storage)
            .save(&vec![CapitalCall {
                subscription: Addr::unchecked("sub_1"),
                amount: 10_000,
            }])
            .unwrap();

        // close call
        let res = execute(
            deps.as_mut(),
            mock_env(),
            mock_info("sub_1", &vec![coin(10_000, "commitment_coin")]),
            HandleMsg::ClaimInvestment { amount: 10_000 },
        );

        assert!(res.is_err());
    }
}

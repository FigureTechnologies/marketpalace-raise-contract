use crate::contract::ContractResponse;
use crate::error::contract_error;
use crate::error::ContractError;
use crate::msg::AcceptSubscription;
use crate::state::config;
use crate::state::{config_read, Status};
use crate::sub_msg::SubTerms;
use crate::sub_msg::{SubExecuteMsg, SubInstantiateMsg, SubQueryMsg};
use cosmwasm_std::coins;
use cosmwasm_std::to_binary;
use cosmwasm_std::wasm_execute;
use cosmwasm_std::Addr;
use cosmwasm_std::Deps;
use cosmwasm_std::DepsMut;
use cosmwasm_std::Env;
use cosmwasm_std::MessageInfo;
use cosmwasm_std::Response;
use cosmwasm_std::StdResult;
use cosmwasm_std::SubMsg;
use cosmwasm_std::WasmMsg;
use provwasm_std::mint_marker_supply;
use provwasm_std::withdraw_coins;
use provwasm_std::ProvenanceQuerier;
use std::collections::HashSet;

pub fn try_propose_subscription(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    min_commitment: u64,
    max_commitment: u64,
    min_days_of_notice: Option<u16>,
) -> ContractResponse {
    let state = config_read(deps.storage).load()?;

    if state.status != Status::Active {
        return contract_error("contract is not active");
    }

    if let Some(min) = state.min_commitment {
        if max_commitment < min {
            return contract_error("subscription max commitment is below raise minumum commitment");
        }
    }

    if let Some(max) = state.max_commitment {
        if min_commitment > max {
            return contract_error("subscription min commitment exceeds raise maximum commitment");
        }
    }

    let create_sub = SubMsg::reply_always(
        WasmMsg::Instantiate {
            admin: Some(env.contract.address.into_string()),
            code_id: state.subscription_code_id,
            msg: to_binary(&SubInstantiateMsg {
                recovery_admin: state.recovery_admin,
                lp: info.sender,
                capital_denom: state.capital_denom,
                min_commitment,
                max_commitment,
                capital_per_share: state.capital_per_share,
                min_days_of_notice,
            })?,
            funds: vec![],
            label: String::from("establish subscription"),
        },
        1,
    );

    Ok(Response::new().add_submessage(create_sub))
}

pub fn try_accept_subscriptions(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    accepts: HashSet<AcceptSubscription>,
) -> ContractResponse {
    let state = config_read(deps.storage).load()?;

    if info.sender != state.gp {
        return contract_error("only gp can accept subscriptions");
    }

    for accept in accepts.iter() {
        let attributes = get_attributes(deps.as_ref(), accept.subscription.clone())?;

        if !accept.is_retroactive {
            if !state.acceptable_accreditations.is_empty()
                && no_matches(&attributes, &state.acceptable_accreditations)
            {
                return contract_error(&format!(
                    "subscription owner must have one of acceptable accreditations: {:?}",
                    state.acceptable_accreditations
                ));
            }

            if missing_any(&attributes, &state.other_required_tags) {
                return contract_error(&format!(
                    "subscription owner must have all other required tags: {:?}",
                    state.other_required_tags
                ));
            }
        }

        if state.not_evenly_divisble(accept.commitment) {
            return contract_error("accept amount must be evenly divisble by capital per share");
        }
    }

    config(deps.storage).update(|mut state| -> Result<_, ContractError> {
        accepts.iter().for_each(|accept| {
            state.pending_review_subs.remove(&accept.subscription);
            state.accepted_subs.insert(accept.subscription.clone());
        });

        Ok(state)
    })?;

    let commitment_total: u64 = accepts.iter().map(|accept| accept.commitment).sum();
    let supply = state.capital_to_shares(commitment_total);
    let mint = mint_marker_supply(supply.into(), state.commitment_denom.clone())?;
    let withdraw = withdraw_coins(
        state.commitment_denom.clone(),
        supply.into(),
        state.commitment_denom.clone(),
        env.contract.address,
    )?;

    Ok(Response::new()
        .add_message(mint)
        .add_message(withdraw)
        .add_messages(accepts.into_iter().map(|accept| {
            wasm_execute(
                accept.subscription,
                &SubExecuteMsg::Accept {},
                coins(
                    state.capital_to_shares(accept.commitment) as u128,
                    state.commitment_denom.clone(),
                ),
            )
            .unwrap()
        })))
}

fn get_attributes(deps: Deps, address: Addr) -> StdResult<HashSet<String>> {
    let terms: SubTerms = deps
        .querier
        .query_wasm_smart(address, &SubQueryMsg::GetTerms {})?;

    Ok(ProvenanceQuerier::new(&deps.querier)
        .get_attributes(terms.lp, None as Option<String>)
        .unwrap()
        .attributes
        .into_iter()
        .map(|attribute| attribute.name)
        .collect())
}

fn no_matches(a: &HashSet<String>, b: &HashSet<String>) -> bool {
    a.intersection(b).count() == 0
}

fn missing_any(a: &HashSet<String>, b: &HashSet<String>) -> bool {
    a.intersection(b).count() != b.len()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::contract::execute;
    use crate::contract::tests::default_deps;
    use crate::mock::mint_args;
    use crate::mock::msg_at_index;
    use crate::mock::wasm_msg;
    use crate::mock::withdraw_args;
    use crate::mock::{wasm_smart_mock_dependencies, MockContractQuerier};
    use crate::msg::HandleMsg;
    use crate::msg::QueryMsg;
    use crate::msg::Subs;
    use crate::query::query;
    use crate::state::State;
    use crate::sub_msg::SubTerms;
    use cosmwasm_std::from_binary;
    use cosmwasm_std::testing::mock_env;
    use cosmwasm_std::testing::mock_info;
    use cosmwasm_std::testing::MockApi;
    use cosmwasm_std::to_binary;
    use cosmwasm_std::ContractResult;
    use cosmwasm_std::MemoryStorage;
    use cosmwasm_std::OwnedDeps;
    use cosmwasm_std::SystemResult;

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

    fn propose_subscription() {
        let mut deps = default_deps(None);

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
        assert_eq!(1, res.messages.len());
    }

    #[test]
    fn propose_subscription_with_max_too_small() {
        let mut deps = default_deps(None);

        // propose a sub as lp
        let res = execute(
            deps.as_mut(),
            mock_env(),
            mock_info("lp", &[]),
            HandleMsg::ProposeSubscription {
                min_commitment: 10_000,
                max_commitment: 5_000,
                min_days_of_notice: None,
            },
        );
        assert!(res.is_err());
    }

    #[test]

    fn propose_subscription_with_min_too_big() {
        let mut deps = default_deps(None);

        // propose a sub as lp
        let res = execute(
            deps.as_mut(),
            mock_env(),
            mock_info("lp", &[]),
            HandleMsg::ProposeSubscription {
                min_commitment: 110_000,
                max_commitment: 100_000,
                min_days_of_notice: None,
            },
        );
        assert!(res.is_err());
    }

    #[test]
    fn accept_subscription() {
        let mut deps = mock_sub_terms();

        let mut state = State::test_default();
        state.pending_review_subs = vec![Addr::unchecked("sub_1")].into_iter().collect();
        config(&mut deps.storage).save(&state).unwrap();

        // accept pending sub as gp
        let res = execute(
            deps.as_mut(),
            mock_env(),
            mock_info("gp", &[]),
            HandleMsg::AcceptSubscriptions {
                subscriptions: vec![AcceptSubscription {
                    subscription: Addr::unchecked("sub_1"),
                    commitment: 20_000,
                    is_retroactive: false,
                }]
                .into_iter()
                .collect(),
            },
        )
        .unwrap();

        // verify that mint, withdraw, and exec message was sent
        assert_eq!(3, res.messages.len());

        // verify minted coin
        let mint = mint_args(msg_at_index(&res, 0));
        assert_eq!(200, mint.amount.u128());
        assert_eq!("commitment_coin", mint.denom);

        // verify withdrawn coin
        let (marker_denom, coin, recipient) = withdraw_args(msg_at_index(&res, 1));
        assert_eq!("commitment_coin", marker_denom);
        assert_eq!(200, coin.amount.u128());
        assert_eq!("commitment_coin", coin.denom);
        assert_eq!("cosmos2contract", recipient.clone().into_string());

        assert!(matches!(
            wasm_msg(msg_at_index(&res, 2)),
            WasmMsg::Execute { .. }
        ));

        // assert that the sub has moved from pending review to accepted
        let res = query(deps.as_ref(), mock_env(), QueryMsg::GetSubs {}).unwrap();
        let subs: Subs = from_binary(&res).unwrap();
        assert_eq!(0, subs.pending_review.len());
        assert_eq!(1, subs.accepted.len());
    }

    #[test]
    fn accept_subscription_bad_actor() {
        let mut deps = mock_sub_terms();

        let mut state = State::test_default();
        state.pending_review_subs = vec![Addr::unchecked("sub_1")].into_iter().collect();
        config(&mut deps.storage).save(&state).unwrap();

        // accept pending sub as gp
        let res = execute(
            deps.as_mut(),
            mock_env(),
            mock_info("bad_actor", &[]),
            HandleMsg::AcceptSubscriptions {
                subscriptions: vec![AcceptSubscription {
                    subscription: Addr::unchecked("sub_1"),
                    commitment: 20_000,
                    is_retroactive: false,
                }]
                .into_iter()
                .collect(),
            },
        );
        assert!(res.is_err());
    }

    #[test]
    fn accept_subscription_missing_acceptable_accreditation() {
        let mut deps = mock_sub_terms();

        let mut state = State::test_default();
        state.pending_review_subs = vec![Addr::unchecked("sub_1")].into_iter().collect();
        state.acceptable_accreditations = vec![String::from("506c")].into_iter().collect();
        config(&mut deps.storage).save(&state).unwrap();

        // accept pending sub as gp
        let res = execute(
            deps.as_mut(),
            mock_env(),
            mock_info("bad_actor", &[]),
            HandleMsg::AcceptSubscriptions {
                subscriptions: vec![AcceptSubscription {
                    subscription: Addr::unchecked("sub_1"),
                    commitment: 20_000,
                    is_retroactive: false,
                }]
                .into_iter()
                .collect(),
            },
        );
        assert!(res.is_err());
    }

    #[test]
    fn accept_subscription_missing_required_tag() {
        let mut deps = mock_sub_terms();

        let mut state = State::test_default();
        state.pending_review_subs = vec![Addr::unchecked("sub_1")].into_iter().collect();
        state.other_required_tags = vec![String::from("misc")].into_iter().collect();
        config(&mut deps.storage).save(&state).unwrap();

        // accept pending sub as gp
        let res = execute(
            deps.as_mut(),
            mock_env(),
            mock_info("bad_actor", &[]),
            HandleMsg::AcceptSubscriptions {
                subscriptions: vec![AcceptSubscription {
                    subscription: Addr::unchecked("sub_1"),
                    commitment: 20_000,
                    is_retroactive: false,
                }]
                .into_iter()
                .collect(),
            },
        );
        assert!(res.is_err());
    }

    #[test]
    fn accept_subscription_with_bad_amount() {
        let mut deps = mock_sub_terms();

        let mut state = State::test_default();
        state.pending_review_subs = vec![Addr::unchecked("sub_1")].into_iter().collect();
        config(&mut deps.storage).save(&state).unwrap();

        // accept pending sub as gp
        let res = execute(
            deps.as_mut(),
            mock_env(),
            mock_info("gp", &[]),
            HandleMsg::AcceptSubscriptions {
                subscriptions: vec![AcceptSubscription {
                    subscription: Addr::unchecked("sub_1"),
                    commitment: 20_001,
                    is_retroactive: false,
                }]
                .into_iter()
                .collect(),
            },
        );
        assert!(res.is_err());
    }
}

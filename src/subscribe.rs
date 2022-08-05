use crate::contract::ContractResponse;
use crate::error::contract_error;
use crate::msg::{AcceptSubscription, AssetExchange};
use crate::state::asset_exchange_storage;
use crate::state::{accepted_subscriptions, config_read, pending_subscriptions};
use crate::sub_msg::{SubInstantiateMsg, SubQueryMsg, SubState};
use cosmwasm_std::DepsMut;
use cosmwasm_std::MessageInfo;
use cosmwasm_std::Response;
use cosmwasm_std::{to_binary, Addr, Env, SubMsg, WasmMsg};
use provwasm_std::ProvenanceQuerier;
use provwasm_std::ProvenanceQuery;
use std::collections::HashSet;
use std::convert::TryInto;

pub fn try_propose_subscription(
    deps: DepsMut<ProvenanceQuery>,
    env: Env,
    info: MessageInfo,
) -> ContractResponse {
    let state = config_read(deps.storage).load()?;

    let create_sub = SubMsg::reply_always(
        WasmMsg::Instantiate {
            admin: Some(env.contract.address.into_string()),
            code_id: state.subscription_code_id,
            msg: to_binary(&SubInstantiateMsg {
                recovery_admin: state.recovery_admin,
                lp: info.sender,
                commitment_denom: state.commitment_denom,
                investment_denom: state.investment_denom,
                capital_denom: state.capital_denom,
                capital_per_share: state.capital_per_share,
            })?,
            funds: vec![],
            label: String::from("establish subscription"),
        },
        1,
    );

    Ok(Response::new().add_submessage(create_sub))
}

pub fn try_close_subscriptions(
    deps: DepsMut<ProvenanceQuery>,
    info: MessageInfo,
    subscriptions: HashSet<Addr>,
) -> ContractResponse {
    let state = config_read(deps.storage).load()?;
    let mut pending = pending_subscriptions(deps.storage)
        .may_load()?
        .unwrap_or_default();
    let mut accepted = accepted_subscriptions(deps.storage)
        .may_load()?
        .unwrap_or_default();

    if info.sender != state.gp {
        return contract_error("only gp can close subscriptions");
    }

    for subscription in subscriptions {
        if !pending.remove(&subscription) {
            if accepted.contains(&subscription) {
                if state.remaining_commitment(deps.as_ref(), &subscription)? == 0 {
                    accepted.remove(&subscription);
                } else {
                    let remaining_commitment = state
                        .remaining_commitment(deps.as_ref(), &subscription)?
                        .try_into()?;
                    asset_exchange_storage(deps.storage).save(
                        subscription.as_bytes(),
                        &vec![AssetExchange {
                            investment: None,
                            commitment: Some(remaining_commitment),
                            capital: None,
                            date: None,
                        }],
                    )?;
                }
            } else {
                return contract_error("no subscription pending or accepted to close");
            }
        }
    }

    pending_subscriptions(deps.storage).save(&pending)?;
    accepted_subscriptions(deps.storage).save(&accepted)?;

    Ok(Response::new())
}

pub fn try_accept_subscriptions(
    deps: DepsMut<ProvenanceQuery>,
    info: MessageInfo,
    accepts: Vec<AcceptSubscription>,
) -> ContractResponse {
    let state = config_read(deps.storage).load()?;
    let mut pending = pending_subscriptions(deps.storage)
        .may_load()?
        .unwrap_or_default();
    let mut accepted = accepted_subscriptions(deps.storage)
        .may_load()?
        .unwrap_or_default();

    if info.sender != state.gp {
        return contract_error("only gp can accept subscriptions");
    }

    for accept in accepts.iter() {
        let sub_state: SubState = deps
            .querier
            .query_wasm_smart(accept.subscription.clone(), &SubQueryMsg::GetState {})?;

        let attributes: HashSet<String> = ProvenanceQuerier::new(&deps.querier)
            .get_attributes(sub_state.lp.clone(), None as Option<String>)
            .unwrap()
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
            return contract_error("subscription owner must have one of acceptable accreditations");
        }

        if state.not_evenly_divisble(accept.commitment) {
            return contract_error("accept amount must be evenly divisble by capital per share");
        }
    }

    for accept in accepts {
        pending.remove(&accept.subscription);
        accepted.insert(accept.subscription.clone());
        asset_exchange_storage(deps.storage).save(
            accept.subscription.as_bytes(),
            &vec![AssetExchange {
                investment: None,
                commitment: Some(accept.commitment.try_into()?),
                capital: None,
                date: None,
            }],
        )?;
    }
    pending_subscriptions(deps.storage).save(&pending)?;
    accepted_subscriptions(deps.storage).save(&accepted)?;

    Ok(Response::default())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::contract::execute;
    use crate::contract::tests::default_deps;
    use crate::mock::{wasm_smart_mock_dependencies, MockContractQuerier};
    use crate::msg::HandleMsg;
    use crate::msg::QueryMsg;
    use crate::msg::RaiseState;
    use crate::query::query;
    use crate::state::accepted_subscriptions_read;
    use crate::state::config;
    use crate::state::pending_subscriptions_read;
    use crate::state::tests::set_pending;
    use crate::state::tests::to_addresses;
    use crate::state::tests::{asset_exchange_storage_read, set_accepted};
    use crate::state::State;
    use cosmwasm_std::coins;
    use cosmwasm_std::from_binary;
    use cosmwasm_std::testing::mock_env;
    use cosmwasm_std::testing::mock_info;
    use cosmwasm_std::testing::MockApi;
    use cosmwasm_std::to_binary;
    use cosmwasm_std::Addr;
    use cosmwasm_std::ContractResult;
    use cosmwasm_std::MemoryStorage;
    use cosmwasm_std::OwnedDeps;
    use cosmwasm_std::SystemResult;

    pub fn mock_sub_state(
    ) -> OwnedDeps<MemoryStorage, MockApi, MockContractQuerier, ProvenanceQuery> {
        wasm_smart_mock_dependencies(&vec![], |_, _| {
            SystemResult::Ok(ContractResult::Ok(
                to_binary(&SubState {
                    admin: Addr::unchecked("marketpalace"),
                    lp: Addr::unchecked("lp"),
                    raise: Addr::unchecked("raise_1"),
                    commitment_denom: String::from("raise_1.commitment"),
                    investment_denom: String::from("raise_1.investment"),
                    capital_denom: String::from("stable_coin"),
                    capital_per_share: 1,
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
            HandleMsg::ProposeSubscription {},
        )
        .unwrap();
        assert_eq!(1, res.messages.len());
    }

    #[test]
    fn close_subscriptions_pending() {
        let mut deps = default_deps(None);
        set_pending(&mut deps.storage, vec!["sub_1"]);

        // close sub as gp
        execute(
            deps.as_mut(),
            mock_env(),
            mock_info("gp", &[]),
            HandleMsg::CloseSubscriptions {
                subscriptions: to_addresses(vec!["sub_1"]),
            },
        )
        .unwrap();

        // verify pending sub is removed
        assert_eq!(
            0,
            pending_subscriptions_read(&deps.storage)
                .load()
                .unwrap()
                .len()
        )
    }

    #[test]
    fn close_subscriptions_accepted_no_commitment() {
        let mut deps = default_deps(None);
        set_accepted(&mut deps.storage, vec!["sub_1"]);

        // close sub as gp
        execute(
            deps.as_mut(),
            mock_env(),
            mock_info("gp", &[]),
            HandleMsg::CloseSubscriptions {
                subscriptions: to_addresses(vec!["sub_1"]),
            },
        )
        .unwrap();

        // verify accepted sub is removed
        assert_eq!(
            0,
            accepted_subscriptions_read(&deps.storage)
                .load()
                .unwrap()
                .len()
        )
    }

    #[test]
    fn close_subscriptions_accepted_commitment() {
        let mut deps = default_deps(None);
        config(&mut deps.storage)
            .save(&&State::test_default())
            .unwrap();
        set_accepted(&mut deps.storage, vec!["sub_1"]);
        deps.querier
            .base
            .update_balance(Addr::unchecked("sub_1"), coins(100, "commitment_coin"));

        // close sub as gp
        execute(
            deps.as_mut(),
            mock_env(),
            mock_info("gp", &[]),
            HandleMsg::CloseSubscriptions {
                subscriptions: to_addresses(vec!["sub_1"]),
            },
        )
        .unwrap();

        // verify accepted sub remains
        assert_eq!(
            1,
            accepted_subscriptions_read(&deps.storage)
                .load()
                .unwrap()
                .len()
        );
        // verify outsounding sub closure exists
        assert_eq!(
            1,
            asset_exchange_storage_read(&deps.storage)
                .load(Addr::unchecked("sub_1").as_bytes())
                .unwrap()
                .len()
        )
    }

    #[test]
    fn close_subscriptions_bad_actor() {
        let mut deps = default_deps(None);
        set_accepted(&mut deps.storage, vec!["sub_1"]);

        // close sub as gp
        let res = execute(
            deps.as_mut(),
            mock_env(),
            mock_info("bad_actor", &[]),
            HandleMsg::CloseSubscriptions {
                subscriptions: to_addresses(vec!["sub_1"]),
            },
        );

        assert!(res.is_err());
    }

    #[test]
    fn close_subscriptions_not_found() {
        let mut deps = default_deps(None);
        set_accepted(&mut deps.storage, vec!["sub_1"]);

        // close sub as gp
        let res = execute(
            deps.as_mut(),
            mock_env(),
            mock_info("gp", &[]),
            HandleMsg::CloseSubscriptions {
                subscriptions: to_addresses(vec!["sub_2"]),
            },
        );

        assert!(res.is_err());
    }

    #[test]
    fn accept_subscription() {
        let mut deps = mock_sub_state();
        config(&mut deps.storage)
            .save(&State::test_default())
            .unwrap();
        set_pending(&mut deps.storage, vec!["sub_1"]);

        // accept pending sub as gp
        execute(
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

        // assert that the sub has moved from pending review to accepted
        let res = query(deps.as_ref(), mock_env(), QueryMsg::GetState {}).unwrap();
        let state: RaiseState = from_binary(&res).unwrap();
        assert_eq!(0, state.pending_subscriptions.len());
        assert_eq!(1, state.accepted_subscriptions.len());

        // verify asset exchange exists
        assert_eq!(
            1,
            asset_exchange_storage_read(&mut deps.storage)
                .load(Addr::unchecked("sub_1").as_bytes())
                .unwrap()
                .len()
        )
    }

    #[test]
    fn accept_subscription_bad_actor() {
        let mut deps = mock_sub_state();
        set_pending(&mut deps.storage, vec!["sub_1"]);

        // accept pending sub as gp
        let res = execute(
            deps.as_mut(),
            mock_env(),
            mock_info("bad_actor", &[]),
            HandleMsg::AcceptSubscriptions {
                subscriptions: vec![AcceptSubscription {
                    subscription: Addr::unchecked("sub_1"),
                    commitment: 20_000,
                }]
                .into_iter()
                .collect(),
            },
        );
        assert!(res.is_err());
    }

    #[test]
    fn accept_subscription_missing_acceptable_accreditation() {
        let mut deps = mock_sub_state();

        let mut state = State::test_default();
        state.acceptable_accreditations = vec![String::from("506c")].into_iter().collect();
        config(&mut deps.storage).save(&state).unwrap();

        set_pending(&mut deps.storage, vec!["sub_1"]);

        // accept pending sub as gp
        let res = execute(
            deps.as_mut(),
            mock_env(),
            mock_info("bad_actor", &[]),
            HandleMsg::AcceptSubscriptions {
                subscriptions: vec![AcceptSubscription {
                    subscription: Addr::unchecked("sub_1"),
                    commitment: 20_000,
                }]
                .into_iter()
                .collect(),
            },
        );
        assert!(res.is_err());
    }

    #[test]
    fn accept_subscription_with_bad_amount() {
        let mut deps = mock_sub_state();
        set_pending(&mut deps.storage, vec!["sub_1"]);

        // accept pending sub as gp
        let res = execute(
            deps.as_mut(),
            mock_env(),
            mock_info("gp", &[]),
            HandleMsg::AcceptSubscriptions {
                subscriptions: vec![AcceptSubscription {
                    subscription: Addr::unchecked("sub_1"),
                    commitment: 20_001,
                }]
                .into_iter()
                .collect(),
            },
        );
        assert!(res.is_err());
    }
}

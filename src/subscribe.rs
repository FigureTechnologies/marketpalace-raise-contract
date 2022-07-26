use crate::contract::ContractResponse;
use crate::error::contract_error;
use crate::msg::{AcceptSubscription, CommitmentUpdate};
use crate::state::{accepted_subscriptions, config_read, pending_subscriptions};
use crate::state::{
    closed_subscriptions, outstanding_commitment_updates, outstanding_subscription_closures,
};
use crate::sub_msg::SubTerms;
use crate::sub_msg::{SubInstantiateMsg, SubQueryMsg};
use cosmwasm_std::Deps;
use cosmwasm_std::DepsMut;
use cosmwasm_std::Env;
use cosmwasm_std::MessageInfo;
use cosmwasm_std::Response;
use cosmwasm_std::StdResult;
use cosmwasm_std::SubMsg;
use cosmwasm_std::WasmMsg;
use cosmwasm_std::{to_binary, Addr};
use provwasm_std::withdraw_coins;
use provwasm_std::ProvenanceQuerier;
use provwasm_std::ProvenanceQuery;
use provwasm_std::{burn_marker_supply, mint_marker_supply};
use std::collections::HashSet;

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
    let mut subscription_closures = outstanding_subscription_closures(deps.storage)
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
                    subscription_closures.insert(subscription);
                }
            } else {
                return contract_error("no subscription pending or accepted to close");
            }
        }
    }

    pending_subscriptions(deps.storage).save(&pending)?;
    accepted_subscriptions(deps.storage).save(&accepted)?;
    outstanding_subscription_closures(deps.storage).save(&subscription_closures)?;

    Ok(Response::new())
}

pub fn try_close_remaining_commitment(
    deps: DepsMut<ProvenanceQuery>,
    info: MessageInfo,
) -> ContractResponse {
    let state = config_read(deps.storage).load()?;

    let mut closures = outstanding_subscription_closures(deps.storage).load()?;
    if !closures.remove(&info.sender) {
        return contract_error("no close for subscription");
    }
    outstanding_subscription_closures(deps.storage).save(&closures)?;

    let mut closed = closed_subscriptions(deps.storage)
        .may_load()?
        .unwrap_or_default();
    closed.insert(info.sender.clone());
    closed_subscriptions(deps.storage).save(&closed)?;

    if state.remaining_commitment(deps.as_ref(), &info.sender)? > 0 {
        return contract_error("commitment balance must be zero");
    }

    let deposit_commitment =
        state.deposit_commitment_msg(deps.as_ref(), info.funds.first().unwrap().amount.u128())?;
    let burn_commitment = burn_marker_supply(
        info.funds.first().unwrap().amount.u128(),
        state.commitment_denom,
    )?;

    Ok(Response::new()
        .add_message(deposit_commitment)
        .add_message(burn_commitment))
}

pub fn try_accept_subscriptions(
    deps: DepsMut<ProvenanceQuery>,
    info: MessageInfo,
    accepts: HashSet<AcceptSubscription>,
) -> ContractResponse {
    let state = config_read(deps.storage).load()?;
    let mut pending = pending_subscriptions(deps.storage)
        .may_load()?
        .unwrap_or_default();
    let mut accepted = accepted_subscriptions(deps.storage)
        .may_load()?
        .unwrap_or_default();
    let mut commitment_updates = outstanding_commitment_updates(deps.storage)
        .may_load()?
        .unwrap_or_default();

    if info.sender != state.gp {
        return contract_error("only gp can accept subscriptions");
    }

    for accept in accepts.iter() {
        let terms: SubTerms = deps
            .querier
            .query_wasm_smart(accept.subscription.clone(), &SubQueryMsg::GetTerms {})?;

        let attributes = get_attributes(deps.as_ref(), &terms)?;

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

        if state.not_evenly_divisble(accept.commitment) {
            return contract_error("accept amount must be evenly divisble by capital per share");
        }
    }

    accepts.iter().for_each(|accept| {
        pending.remove(&accept.subscription);
        accepted.insert(accept.subscription.clone());
        commitment_updates.insert(CommitmentUpdate {
            subscription: accept.subscription.clone(),
            change_by_amount: accept.commitment as i64,
        });
    });
    pending_subscriptions(deps.storage).save(&pending)?;
    accepted_subscriptions(deps.storage).save(&accepted)?;
    outstanding_commitment_updates(deps.storage).save(&commitment_updates)?;

    Ok(Response::default())
}

pub fn try_update_commitments(
    deps: DepsMut<ProvenanceQuery>,
    info: MessageInfo,
    updates: HashSet<CommitmentUpdate>,
) -> ContractResponse {
    let state = config_read(deps.storage).load()?;
    let accepted = accepted_subscriptions(deps.storage)
        .may_load()?
        .unwrap_or_default();

    if info.sender != state.gp {
        return contract_error("only gp can update commitments");
    }

    let mut commitment_updates = outstanding_commitment_updates(deps.storage)
        .may_load()?
        .unwrap_or_default();

    for update in updates {
        if !accepted.contains(&update.subscription) {
            return contract_error("no subscription accepted to update commitment");
        }

        if update.change_by_amount == 0 {
            return contract_error("commitment update must be non zero");
        }

        let remaining = state.remaining_commitment(deps.as_ref(), &update.subscription)?;
        if update.change_by_amount < 0 && remaining < update.change_by_amount.unsigned_abs().into()
        {
            return contract_error("can not have a negative remaining commitment");
        }

        commitment_updates.insert(update);
    }

    outstanding_commitment_updates(deps.storage).save(&commitment_updates)?;

    Ok(Response::default())
}

pub fn try_accept_commitment_update(
    deps: DepsMut<ProvenanceQuery>,
    info: MessageInfo,
) -> ContractResponse {
    let state = config_read(deps.storage).load()?;

    let mut commitment_updates = outstanding_commitment_updates(deps.storage)
        .may_load()?
        .unwrap_or_default();

    let update = commitment_updates
        .take(&CommitmentUpdate {
            subscription: info.sender.clone(),
            change_by_amount: 0,
        })
        .ok_or("no update found")?;
    outstanding_commitment_updates(deps.storage).save(&commitment_updates)?;

    let abs_amount = update.change_by_amount.unsigned_abs();
    if update.change_by_amount > 0 {
        Ok(Response::new().add_messages(vec![
            mint_marker_supply(abs_amount.into(), state.commitment_denom.clone())?,
            withdraw_coins(
                state.commitment_denom.clone(),
                abs_amount.into(),
                state.commitment_denom,
                info.sender,
            )?,
        ]))
    } else {
        let sent = info.funds.first().ok_or("commitment required")?;

        if sent.denom != state.commitment_denom {
            return contract_error("funds must be commitment denom");
        }

        if sent.amount.u128() != abs_amount.into() {
            return contract_error("funds must match update amount");
        }

        Ok(Response::new()
            .add_message(state.deposit_commitment_msg(deps.as_ref(), abs_amount.into())?)
            .add_message(burn_marker_supply(
                abs_amount.into(),
                state.commitment_denom,
            )?))
    }
}

fn get_attributes(deps: Deps<ProvenanceQuery>, terms: &SubTerms) -> StdResult<HashSet<String>> {
    Ok(ProvenanceQuerier::new(&deps.querier)
        .get_attributes(terms.lp.clone(), None as Option<String>)
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
    use crate::mock::burn_args;
    use crate::mock::load_markers;
    use crate::mock::mint_args;
    use crate::mock::msg_at_index;
    use crate::mock::send_args;
    use crate::mock::withdraw_args;
    use crate::mock::{wasm_smart_mock_dependencies, MockContractQuerier};
    use crate::msg::HandleMsg;
    use crate::msg::QueryMsg;
    use crate::msg::RaiseState;
    use crate::query::query;
    use crate::state::accepted_subscriptions_read;
    use crate::state::config;
    use crate::state::pending_subscriptions_read;
    use crate::state::tests::set_accepted;
    use crate::state::tests::set_pending;
    use crate::state::tests::to_addresses;
    use crate::state::State;
    use crate::sub_msg::SubTerms;
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

    pub fn mock_sub_terms(
    ) -> OwnedDeps<MemoryStorage, MockApi, MockContractQuerier, ProvenanceQuery> {
        wasm_smart_mock_dependencies(&vec![], |_, _| {
            SystemResult::Ok(ContractResult::Ok(
                to_binary(&SubTerms {
                    lp: Addr::unchecked("lp"),
                    raise: Addr::unchecked("raise_1"),
                    capital_denom: String::from("stable_coin"),
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
            outstanding_subscription_closures(&mut deps.storage)
                .load()
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
    fn close_remaining_commitment() {
        let mut deps = default_deps(None);
        load_markers(&mut deps.querier);
        outstanding_subscription_closures(&mut deps.storage)
            .save(&to_addresses(vec!["sub_1"]))
            .unwrap();

        // close remaining commitment
        let res = execute(
            deps.as_mut(),
            mock_env(),
            mock_info("sub_1", &coins(100, "commitment_coin")),
            HandleMsg::CloseRemainingCommitment {},
        )
        .unwrap();

        assert_eq!(2, res.messages.len());

        // verify deposit commitment
        let (to_address, coins) = send_args(msg_at_index(&res, 0));
        assert_eq!("tp18vmzryrvwaeykmdtu6cfrz5sau3dhc5c73ms0u", to_address);
        assert_eq!("commitment_coin", coins.first().unwrap().denom);
        assert_eq!(100, coins.first().unwrap().amount.u128());

        // verify burn commitment
        let coin = burn_args(msg_at_index(&res, 1));
        assert_eq!("commitment_coin", coin.denom);
        assert_eq!(100, coin.amount.u128());

        // verify outsounding sub closure removed
        assert_eq!(
            0,
            outstanding_subscription_closures(&mut deps.storage)
                .load()
                .unwrap()
                .len()
        );
        // verify closed sub exists
        assert_eq!(
            1,
            closed_subscriptions(&mut deps.storage)
                .load()
                .unwrap()
                .len()
        )
    }

    #[test]
    fn close_remaining_commitment_not_found() {
        let mut deps = default_deps(None);
        outstanding_subscription_closures(&mut deps.storage)
            .save(&to_addresses(vec!["sub_1"]))
            .unwrap();

        // close remaining commitment
        let res = execute(
            deps.as_mut(),
            mock_env(),
            mock_info("sub_2", &coins(100, "commitment_coin")),
            HandleMsg::CloseRemainingCommitment {},
        );

        assert!(res.is_err());
    }

    #[test]
    fn close_remaining_commitment_leftover() {
        let mut deps = default_deps(None);
        outstanding_subscription_closures(&mut deps.storage)
            .save(&to_addresses(vec!["sub_1"]))
            .unwrap();
        deps.querier
            .base
            .update_balance(Addr::unchecked("sub_1"), coins(100, "commitment_coin"));

        // close remaining commitment
        let res = execute(
            deps.as_mut(),
            mock_env(),
            mock_info("sub_1", &coins(100, "commitment_coin")),
            HandleMsg::CloseRemainingCommitment {},
        );

        assert!(res.is_err());
    }

    #[test]
    fn accept_subscription() {
        let mut deps = mock_sub_terms();
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

        // verify outsounding commitment update exists
        assert_eq!(
            1,
            outstanding_commitment_updates(&mut deps.storage)
                .load()
                .unwrap()
                .len()
        )
    }

    #[test]
    fn accept_subscription_bad_actor() {
        let mut deps = mock_sub_terms();
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
        let mut deps = mock_sub_terms();

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
    fn accept_subscription_missing_required_tag() {
        let mut deps = mock_sub_terms();

        let mut state = State::test_default();
        state.other_required_tags = vec![String::from("misc")].into_iter().collect();
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
        let mut deps = mock_sub_terms();
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

    #[test]
    fn update_commitments_increase() {
        let mut deps = default_deps(None);
        set_accepted(&mut deps.storage, vec!["sub_1"]);
        deps.querier
            .base
            .update_balance(Addr::unchecked("sub_1"), coins(1_000, "commitment_coin"));

        // update commitments
        execute(
            deps.as_mut(),
            mock_env(),
            mock_info("gp", &[]),
            HandleMsg::UpdateCommitments {
                commitment_updates: vec![CommitmentUpdate {
                    subscription: Addr::unchecked("sub_1"),
                    change_by_amount: 1_000,
                }]
                .into_iter()
                .collect(),
            },
        )
        .unwrap();

        // verify outsounding commitment update exists
        assert_eq!(
            1,
            outstanding_commitment_updates(&mut deps.storage)
                .load()
                .unwrap()
                .len()
        )
    }

    #[test]
    fn update_commitments_decrease() {
        let mut deps = default_deps(None);
        set_accepted(&mut deps.storage, vec!["sub_1"]);
        deps.querier
            .base
            .update_balance(Addr::unchecked("sub_1"), coins(1_000, "commitment_coin"));

        // update commitments
        execute(
            deps.as_mut(),
            mock_env(),
            mock_info("gp", &[]),
            HandleMsg::UpdateCommitments {
                commitment_updates: vec![CommitmentUpdate {
                    subscription: Addr::unchecked("sub_1"),
                    change_by_amount: -1_000,
                }]
                .into_iter()
                .collect(),
            },
        )
        .unwrap();

        // verify outsounding commitment update exists
        assert_eq!(
            1,
            outstanding_commitment_updates(&mut deps.storage)
                .load()
                .unwrap()
                .len()
        )
    }

    #[test]
    fn update_commitments_decrease_too_much() {
        let mut deps = default_deps(None);
        set_pending(&mut deps.storage, vec!["sub_1"]);
        deps.querier
            .base
            .update_balance(Addr::unchecked("sub_1"), coins(1_000, "commitment_coin"));

        // update commitments
        let res = execute(
            deps.as_mut(),
            mock_env(),
            mock_info("gp", &[]),
            HandleMsg::UpdateCommitments {
                commitment_updates: vec![CommitmentUpdate {
                    subscription: Addr::unchecked("sub_1"),
                    change_by_amount: -2_000,
                }]
                .into_iter()
                .collect(),
            },
        );

        assert!(res.is_err());
    }

    #[test]
    fn update_commitments_decrease_zero() {
        let mut deps = default_deps(None);
        set_accepted(&mut deps.storage, vec!["sub_1"]);
        deps.querier
            .base
            .update_balance(Addr::unchecked("sub_1"), coins(1_000, "commitment_coin"));

        // update commitments
        let res = execute(
            deps.as_mut(),
            mock_env(),
            mock_info("gp", &[]),
            HandleMsg::UpdateCommitments {
                commitment_updates: vec![CommitmentUpdate {
                    subscription: Addr::unchecked("sub_1"),
                    change_by_amount: 0,
                }]
                .into_iter()
                .collect(),
            },
        );

        assert!(res.is_err());
    }

    #[test]
    fn update_commitments_bad_actor() {
        let mut deps = default_deps(None);
        set_accepted(&mut deps.storage, vec!["sub_1"]);
        deps.querier
            .base
            .update_balance(Addr::unchecked("sub_1"), coins(1_000, "commitment_coin"));

        // update commitments
        let res = execute(
            deps.as_mut(),
            mock_env(),
            mock_info("bad_actor", &[]),
            HandleMsg::UpdateCommitments {
                commitment_updates: vec![CommitmentUpdate {
                    subscription: Addr::unchecked("sub_1"),
                    change_by_amount: 1_000,
                }]
                .into_iter()
                .collect(),
            },
        );

        assert!(res.is_err());
    }

    #[test]
    fn accept_commitment_update_increase() {
        let mut deps = default_deps(None);
        load_markers(&mut deps.querier);
        outstanding_commitment_updates(&mut deps.storage)
            .save(
                &vec![CommitmentUpdate {
                    subscription: Addr::unchecked("sub_1"),
                    change_by_amount: 1_000,
                }]
                .into_iter()
                .collect(),
            )
            .unwrap();

        // accept commitment update
        let res = execute(
            deps.as_mut(),
            mock_env(),
            mock_info("sub_1", &vec![]),
            HandleMsg::AcceptCommitmentUpdate {},
        )
        .unwrap();

        assert_eq!(2, res.messages.len());

        // verify minted coin
        let mint = mint_args(msg_at_index(&res, 0));
        assert_eq!(1_000, mint.amount.u128());
        assert_eq!("commitment_coin", mint.denom);

        // verify withdrawn coin
        let (marker_denom, coin, recipient) = withdraw_args(msg_at_index(&res, 1));
        assert_eq!("commitment_coin", marker_denom);
        assert_eq!(1_000, coin.amount.u128());
        assert_eq!("commitment_coin", coin.denom);
        assert_eq!("sub_1", recipient.clone().into_string());

        // verify outsounding commitment update removed
        assert_eq!(
            0,
            outstanding_commitment_updates(&mut deps.storage)
                .load()
                .unwrap()
                .len()
        );
    }

    #[test]
    fn accept_commitment_update_decrease() {
        let mut deps = default_deps(None);
        load_markers(&mut deps.querier);
        outstanding_commitment_updates(&mut deps.storage)
            .save(
                &vec![CommitmentUpdate {
                    subscription: Addr::unchecked("sub_1"),
                    change_by_amount: -1_000,
                }]
                .into_iter()
                .collect(),
            )
            .unwrap();

        // accept commitment update
        let res = execute(
            deps.as_mut(),
            mock_env(),
            mock_info("sub_1", &coins(1_000, "commitment_coin")),
            HandleMsg::AcceptCommitmentUpdate {},
        )
        .unwrap();

        assert_eq!(2, res.messages.len());

        // verify deposit commitment
        let (to_address, coins) = send_args(msg_at_index(&res, 0));
        assert_eq!("tp18vmzryrvwaeykmdtu6cfrz5sau3dhc5c73ms0u", to_address);
        assert_eq!("commitment_coin", coins.first().unwrap().denom);
        assert_eq!(1_000, coins.first().unwrap().amount.u128());

        // verify burn commitment
        let coin = burn_args(msg_at_index(&res, 1));
        assert_eq!("commitment_coin", coin.denom);
        assert_eq!(1_000, coin.amount.u128());

        // verify outsounding commitment update removed
        assert_eq!(
            0,
            outstanding_commitment_updates(&mut deps.storage)
                .load()
                .unwrap()
                .len()
        );
    }

    #[test]
    fn accept_commitment_update_decrease_no_funds() {
        let mut deps = default_deps(None);
        load_markers(&mut deps.querier);
        outstanding_commitment_updates(&mut deps.storage)
            .save(
                &vec![CommitmentUpdate {
                    subscription: Addr::unchecked("sub_1"),
                    change_by_amount: -1_000,
                }]
                .into_iter()
                .collect(),
            )
            .unwrap();

        // accept commitment update
        let res = execute(
            deps.as_mut(),
            mock_env(),
            mock_info("sub_1", &vec![]),
            HandleMsg::AcceptCommitmentUpdate {},
        );

        assert!(res.is_err())
    }

    #[test]
    fn accept_commitment_update_decrease_bad_denom() {
        let mut deps = default_deps(None);
        load_markers(&mut deps.querier);
        outstanding_commitment_updates(&mut deps.storage)
            .save(
                &vec![CommitmentUpdate {
                    subscription: Addr::unchecked("sub_1"),
                    change_by_amount: -1_000,
                }]
                .into_iter()
                .collect(),
            )
            .unwrap();

        // accept commitment update
        let res = execute(
            deps.as_mut(),
            mock_env(),
            mock_info("sub_1", &coins(1_000, "bad_denom")),
            HandleMsg::AcceptCommitmentUpdate {},
        );

        assert!(res.is_err())
    }

    #[test]
    fn accept_commitment_update_decrease_bad_amount() {
        let mut deps = default_deps(None);
        load_markers(&mut deps.querier);
        outstanding_commitment_updates(&mut deps.storage)
            .save(
                &vec![CommitmentUpdate {
                    subscription: Addr::unchecked("sub_1"),
                    change_by_amount: -1_000,
                }]
                .into_iter()
                .collect(),
            )
            .unwrap();

        // accept commitment update
        let res = execute(
            deps.as_mut(),
            mock_env(),
            mock_info("sub_1", &coins(100, "commitment_denom")),
            HandleMsg::AcceptCommitmentUpdate {},
        );

        assert!(res.is_err())
    }
}

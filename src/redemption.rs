use std::collections::HashSet;

use cosmwasm_std::{coins, Addr, BankMsg, DepsMut, Env, MessageInfo, Response};
use provwasm_std::{burn_marker_supply, ProvenanceQuerier, ProvenanceQuery};

use crate::{
    contract::ContractResponse,
    error::contract_error,
    msg::Redemption,
    state::{accepted_subscriptions, config_read, outstanding_redemptions},
};

pub fn try_issue_redemptions(
    deps: DepsMut<ProvenanceQuery>,
    info: MessageInfo,
    redemptions: HashSet<Redemption>,
) -> ContractResponse {
    let state = config_read(deps.storage).load()?;
    let accepted = accepted_subscriptions(deps.storage)
        .may_load()?
        .unwrap_or_default();

    if info.sender != state.gp {
        return contract_error("only gp can issue redemptions");
    }

    if redemptions
        .iter()
        .any(|redemption| !accepted.contains(&redemption.subscription))
    {
        return contract_error("subscription not accepted");
    }

    let existing_redemptions = outstanding_redemptions(deps.storage)
        .may_load()?
        .unwrap_or_default();
    let redemptions = existing_redemptions.union(&redemptions).cloned().collect();

    outstanding_redemptions(deps.storage).save(&redemptions)?;

    Ok(Response::default())
}

pub fn try_cancel_redemptions(
    deps: DepsMut<ProvenanceQuery>,
    info: MessageInfo,
    subscriptions: HashSet<Addr>,
) -> ContractResponse {
    let state = config_read(deps.storage).load()?;

    if info.sender != state.gp {
        return contract_error("only gp can cancel redemptions");
    }

    if let Some(mut existing) = outstanding_redemptions(deps.storage).may_load()? {
        for subscription in subscriptions {
            existing
                .take(&Redemption {
                    subscription,
                    asset: 0,
                    capital: 0,
                    available_epoch_seconds: None,
                })
                .ok_or("no redemption found")?;
        }

        outstanding_redemptions(deps.storage).save(&existing)?;
    } else {
        return contract_error("no outstanding redemptions to cancel");
    };

    Ok(Response::default())
}

pub fn try_claim_redemption(
    deps: DepsMut<ProvenanceQuery>,
    env: Env,
    info: MessageInfo,
    to: Addr,
    memo: Option<String>,
) -> ContractResponse {
    let state = config_read(deps.storage).load()?;

    let mut redemptions = outstanding_redemptions(deps.storage).load()?;
    let redemption = redemptions
        .take(&Redemption {
            subscription: info.sender,
            asset: 0,
            capital: 0,
            available_epoch_seconds: None,
        })
        .ok_or("no redemption found")?;

    if let Some(available) = redemption.available_epoch_seconds {
        if available > env.block.time.seconds() {
            return contract_error("redemption not yet available");
        }
    }

    let sent = info.funds.first().ok_or("asset required for redemption")?;

    if sent.denom != state.investment_denom {
        return contract_error("payment should be made in investment denom");
    }

    if sent.amount.u128() != redemption.asset.into() {
        return contract_error("sent funds should match specified asset");
    }

    outstanding_redemptions(deps.storage).save(&redemptions)?;

    let send = BankMsg::Send {
        to_address: to.into_string(),
        amount: coins(redemption.capital as u128, state.capital_denom),
    };

    let investment_marker = ProvenanceQuerier::new(&deps.querier)
        .get_marker_by_denom(state.investment_denom.clone())?;
    let deposit_investment = BankMsg::Send {
        to_address: investment_marker.address.into_string(),
        amount: coins(redemption.asset.into(), state.investment_denom.clone()),
    };
    let burn_investment = burn_marker_supply(redemption.asset.into(), state.investment_denom)?;

    let msg = Response::new()
        .add_message(send)
        .add_message(deposit_investment)
        .add_message(burn_investment);
    Ok(match memo {
        Some(memo) => msg.add_attribute(String::from("memo"), memo),
        None => msg,
    })
}

#[cfg(test)]
pub mod tests {
    use super::*;
    use crate::contract::execute;
    use crate::contract::tests::default_deps;
    use crate::mock::burn_args;
    use crate::mock::load_markers;
    use crate::mock::msg_at_index;
    use crate::mock::send_args;
    use crate::msg::HandleMsg;
    use crate::state::tests::set_accepted;
    use crate::state::tests::to_addresses;
    use cosmwasm_std::testing::{mock_env, mock_info};
    use cosmwasm_std::Addr;
    use cosmwasm_std::Timestamp;

    #[test]
    fn issue_redemptions() {
        let mut deps = default_deps(None);
        set_accepted(&mut deps.storage, vec!["sub_2"]);
        outstanding_redemptions(&mut deps.storage)
            .save(
                &vec![Redemption {
                    subscription: Addr::unchecked("sub_1"),
                    capital: 10_000,
                    asset: 5_000,
                    available_epoch_seconds: None,
                }]
                .into_iter()
                .collect(),
            )
            .unwrap();

        execute(
            deps.as_mut(),
            mock_env(),
            mock_info("gp", &vec![]),
            HandleMsg::IssueRedemptions {
                redemptions: vec![Redemption {
                    subscription: Addr::unchecked("sub_2"),
                    capital: 10_000,
                    asset: 5_000,
                    available_epoch_seconds: None,
                }]
                .into_iter()
                .collect(),
            },
        )
        .unwrap();

        // verify redemption is saved
        assert_eq!(
            2,
            outstanding_redemptions(&mut deps.storage)
                .load()
                .unwrap()
                .len()
        )
    }

    #[test]
    fn issue_redemptions_bad_actor() {
        let res = execute(
            default_deps(None).as_mut(),
            mock_env(),
            mock_info("bad_actor", &coins(10_000, "stable_coin")),
            HandleMsg::IssueRedemptions {
                redemptions: HashSet::new(),
            },
        );

        assert!(res.is_err());
    }

    #[test]
    fn issue_redemptions_not_accepted() {
        let res = execute(
            default_deps(None).as_mut(),
            mock_env(),
            mock_info("gp", &coins(10_000, "stable_coin")),
            HandleMsg::IssueRedemptions {
                redemptions: vec![Redemption {
                    subscription: Addr::unchecked("sub_1"),
                    asset: 100,
                    capital: 10_000,
                    available_epoch_seconds: None,
                }]
                .into_iter()
                .collect(),
            },
        );

        assert!(res.is_err());
    }

    #[test]
    fn cancel_redemptions() {
        let mut deps = default_deps(None);
        outstanding_redemptions(&mut deps.storage)
            .save(
                &vec![Redemption {
                    subscription: Addr::unchecked("sub_1"),
                    capital: 10_000,
                    asset: 5_000,
                    available_epoch_seconds: None,
                }]
                .into_iter()
                .collect(),
            )
            .unwrap();

        execute(
            deps.as_mut(),
            mock_env(),
            mock_info("gp", &vec![]),
            HandleMsg::CancelRedemptions {
                subscriptions: to_addresses(vec!["sub_1"]),
            },
        )
        .unwrap();

        // verify redemption is removed
        assert_eq!(
            0,
            outstanding_redemptions(&mut deps.storage)
                .load()
                .unwrap()
                .len()
        )
    }

    #[test]
    fn cancel_redemptions_bad_actor() {
        let res = execute(
            default_deps(None).as_mut(),
            mock_env(),
            mock_info("bad_actor", &coins(10_000, "stable_coin")),
            HandleMsg::CancelRedemptions {
                subscriptions: HashSet::new(),
            },
        );

        assert!(res.is_err());
    }

    #[test]
    fn cancel_redemptions_missing() {
        let res = execute(
            default_deps(None).as_mut(),
            mock_env(),
            mock_info("gp", &coins(10_000, "stable_coin")),
            HandleMsg::CancelRedemptions {
                subscriptions: HashSet::new(),
            },
        );

        assert!(res.is_err());
    }

    #[test]
    fn cancel_redemptions_not_found() {
        let mut deps = default_deps(None);
        outstanding_redemptions(&mut deps.storage)
            .save(&HashSet::new())
            .unwrap();

        let res = execute(
            default_deps(None).as_mut(),
            mock_env(),
            mock_info("gp", &coins(10_000, "stable_coin")),
            HandleMsg::CancelRedemptions {
                subscriptions: to_addresses(vec!["sub_1"]),
            },
        );

        assert!(res.is_err());
    }

    #[test]
    fn claim_redemption() {
        let mut deps = default_deps(None);
        load_markers(&mut deps.querier);
        outstanding_redemptions(&mut deps.storage)
            .save(
                &vec![
                    Redemption {
                        subscription: Addr::unchecked("sub_1"),
                        capital: 10_000,
                        asset: 5_000,
                        available_epoch_seconds: None,
                    },
                    Redemption {
                        subscription: Addr::unchecked("sub_2"),
                        capital: 10_000,
                        asset: 5_000,
                        available_epoch_seconds: None,
                    },
                ]
                .into_iter()
                .collect(),
            )
            .unwrap();

        let res = execute(
            deps.as_mut(),
            mock_env(),
            mock_info("sub_1", &coins(5_000, "investment_coin")),
            HandleMsg::ClaimRedemption {
                to: Addr::unchecked("destination"),
                memo: Some(String::from("note")),
            },
        )
        .unwrap();

        assert_eq!(3, res.messages.len());

        // verify send message
        let (to_address, coins) = send_args(msg_at_index(&res, 0));
        assert_eq!("destination", to_address);
        assert_eq!(10_000, coins.first().unwrap().amount.u128());

        // verify memo
        assert_eq!(1, res.attributes.len());
        let attribute = res.attributes.get(0).unwrap();
        assert_eq!("memo", attribute.key);
        assert_eq!("note", attribute.value);

        // verify deposit investment
        let (to_address, coins) = send_args(msg_at_index(&res, 1));
        assert_eq!("tp18vd8fpwxzck93qlwghaj6arh4p7c5n89x8kskz", to_address);
        assert_eq!("investment_coin", coins.first().unwrap().denom);
        assert_eq!(5_000, coins.first().unwrap().amount.u128());

        // verify burn investment
        let coin = burn_args(msg_at_index(&res, 2));
        assert_eq!("investment_coin", coin.denom);
        assert_eq!(5_000, coin.amount.u128());

        // verify redemption is removed
        assert_eq!(
            1,
            outstanding_redemptions(&mut deps.storage)
                .load()
                .unwrap()
                .len()
        )
    }

    #[test]
    fn claim_redemption_without_asset() {
        let mut deps = default_deps(None);
        outstanding_redemptions(&mut deps.storage)
            .save(
                &vec![Redemption {
                    subscription: Addr::unchecked("sub_1"),
                    capital: 10_000,
                    asset: 5_000,
                    available_epoch_seconds: None,
                }]
                .into_iter()
                .collect(),
            )
            .unwrap();

        let res = execute(
            deps.as_mut(),
            mock_env(),
            mock_info("sub_1", &vec![]),
            HandleMsg::ClaimRedemption {
                to: Addr::unchecked("destination"),
                memo: Some(String::from("note")),
            },
        );

        assert!(res.is_err());
    }

    #[test]
    fn claim_redemption_not_available_yet() {
        let mut deps = default_deps(None);
        load_markers(&mut deps.querier);
        outstanding_redemptions(&mut deps.storage)
            .save(
                &vec![Redemption {
                    subscription: Addr::unchecked("sub_1"),
                    capital: 10_000,
                    asset: 5_000,
                    available_epoch_seconds: Some(1675209600), // Feb 01 2023 UTC
                }]
                .into_iter()
                .collect(),
            )
            .unwrap();
        let mut env = mock_env();
        env.block.time = Timestamp::from_seconds(1672531200); // Jan 01 2023 UTC

        let res = execute(
            deps.as_mut(),
            mock_env(),
            mock_info("sub_1", &coins(5_000, "investment_coin")),
            HandleMsg::ClaimRedemption {
                to: Addr::unchecked("destination"),
                memo: Some(String::from("note")),
            },
        );

        assert!(res.is_err());
    }
}

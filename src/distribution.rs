use std::collections::HashSet;

use cosmwasm_std::{coins, Addr, BankMsg, DepsMut, Env, MessageInfo, Response};
use provwasm_std::ProvenanceQuery;

use crate::{
    contract::ContractResponse,
    error::contract_error,
    msg::Distribution,
    state::{accepted_subscriptions, config_read, outstanding_distributions},
};

pub fn try_issue_distributions(
    deps: DepsMut<ProvenanceQuery>,
    info: MessageInfo,
    distributions: HashSet<Distribution>,
) -> ContractResponse {
    let state = config_read(deps.storage).load()?;
    let accepted = accepted_subscriptions(deps.storage)
        .may_load()?
        .unwrap_or_default();

    if info.sender != state.gp {
        return contract_error("only gp can issue distributions");
    }

    if distributions
        .iter()
        .any(|distribution| !accepted.contains(&distribution.subscription))
    {
        return contract_error("subscription not accepted");
    }

    let existing_distributions = outstanding_distributions(deps.storage)
        .may_load()?
        .unwrap_or_default();
    let distributions = existing_distributions
        .union(&distributions)
        .cloned()
        .collect();

    outstanding_distributions(deps.storage).save(&distributions)?;

    Ok(Response::default())
}

pub fn try_cancel_distributions(
    deps: DepsMut<ProvenanceQuery>,
    info: MessageInfo,
    subscriptions: HashSet<Addr>,
) -> ContractResponse {
    let state = config_read(deps.storage).load()?;

    if info.sender != state.gp {
        return contract_error("only gp can cancel distributions");
    }

    if let Some(mut existing) = outstanding_distributions(deps.storage).may_load()? {
        for subscription in subscriptions {
            existing
                .take(&Distribution {
                    subscription,
                    amount: 0,
                    available_epoch_seconds: None,
                })
                .ok_or("no distribution found")?;
        }

        outstanding_distributions(deps.storage).save(&existing)?;
    } else {
        return contract_error("no outstanding distributions to cancel");
    };

    Ok(Response::default())
}

pub fn try_claim_distribution(
    deps: DepsMut<ProvenanceQuery>,
    env: Env,
    info: MessageInfo,
    to: Addr,
    memo: Option<String>,
) -> ContractResponse {
    let state = config_read(deps.storage).load()?;

    let mut distributions = outstanding_distributions(deps.storage).load()?;
    let distribution = distributions
        .take(&Distribution {
            subscription: info.sender,
            amount: 0,
            available_epoch_seconds: None,
        })
        .ok_or("no distribution found")?;

    if let Some(available) = distribution.available_epoch_seconds {
        if available > env.block.time.seconds() {
            return contract_error("distribution not yet available");
        }
    }

    outstanding_distributions(deps.storage).save(&distributions)?;

    let send = BankMsg::Send {
        to_address: to.into_string(),
        amount: coins(distribution.amount as u128, state.capital_denom),
    };

    let msg = Response::new().add_message(send);
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
    use crate::mock::msg_at_index;
    use crate::mock::send_args;
    use crate::msg::HandleMsg;
    use crate::state::tests::set_accepted;
    use crate::state::tests::to_addresses;
    use cosmwasm_std::testing::{mock_env, mock_info};
    use cosmwasm_std::Addr;
    use cosmwasm_std::Timestamp;

    #[test]
    fn issue_distributions() {
        let mut deps = default_deps(None);
        set_accepted(&mut deps.storage, vec!["sub_2"]);
        outstanding_distributions(&mut deps.storage)
            .save(
                &vec![Distribution {
                    subscription: Addr::unchecked("sub_1"),
                    amount: 10_000,
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
            HandleMsg::IssueDistributions {
                distributions: vec![Distribution {
                    subscription: Addr::unchecked("sub_2"),
                    amount: 10_000,
                    available_epoch_seconds: None,
                }]
                .into_iter()
                .collect(),
            },
        )
        .unwrap();

        // verify distribution is saved
        assert_eq!(
            2,
            outstanding_distributions(&mut deps.storage)
                .load()
                .unwrap()
                .len()
        )
    }

    #[test]
    fn issue_distributions_bad_actor() {
        let res = execute(
            default_deps(None).as_mut(),
            mock_env(),
            mock_info("bad_actor", &coins(10_000, "stable_coin")),
            HandleMsg::IssueDistributions {
                distributions: HashSet::new(),
            },
        );

        assert!(res.is_err());
    }

    #[test]
    fn issue_distributions_not_accepted() {
        let res = execute(
            default_deps(None).as_mut(),
            mock_env(),
            mock_info("gp", &coins(10_000, "stable_coin")),
            HandleMsg::IssueDistributions {
                distributions: vec![Distribution {
                    subscription: Addr::unchecked("sub_1"),
                    amount: 10_000,
                    available_epoch_seconds: None,
                }]
                .into_iter()
                .collect(),
            },
        );

        assert!(res.is_err());
    }

    #[test]
    fn cancel_distributions() {
        let mut deps = default_deps(None);
        outstanding_distributions(&mut deps.storage)
            .save(
                &vec![Distribution {
                    subscription: Addr::unchecked("sub_1"),
                    amount: 10_000,
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
            HandleMsg::CancelDistributions {
                subscriptions: to_addresses(vec!["sub_1"]),
            },
        )
        .unwrap();

        // verify distribution is removed
        assert_eq!(
            0,
            outstanding_distributions(&mut deps.storage)
                .load()
                .unwrap()
                .len()
        )
    }

    #[test]
    fn cancel_distributions_bad_actor() {
        let res = execute(
            default_deps(None).as_mut(),
            mock_env(),
            mock_info("bad_actor", &coins(10_000, "stable_coin")),
            HandleMsg::CancelDistributions {
                subscriptions: HashSet::new(),
            },
        );

        assert!(res.is_err());
    }

    #[test]
    fn cancel_distributions_missing() {
        let res = execute(
            default_deps(None).as_mut(),
            mock_env(),
            mock_info("gp", &coins(10_000, "stable_coin")),
            HandleMsg::CancelDistributions {
                subscriptions: HashSet::new(),
            },
        );

        assert!(res.is_err());
    }

    #[test]
    fn cancel_distributions_not_found() {
        let mut deps = default_deps(None);
        outstanding_distributions(&mut deps.storage)
            .save(&HashSet::new())
            .unwrap();

        let res = execute(
            default_deps(None).as_mut(),
            mock_env(),
            mock_info("gp", &coins(10_000, "stable_coin")),
            HandleMsg::CancelDistributions {
                subscriptions: to_addresses(vec!["sub_1"]),
            },
        );

        assert!(res.is_err());
    }

    #[test]
    fn claim_distribution() {
        let mut deps = default_deps(None);
        outstanding_distributions(&mut deps.storage)
            .save(
                &vec![
                    Distribution {
                        subscription: Addr::unchecked("sub_1"),
                        amount: 10_000,
                        available_epoch_seconds: None,
                    },
                    Distribution {
                        subscription: Addr::unchecked("sub_2"),
                        amount: 10_000,
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
            mock_info("sub_1", &vec![]),
            HandleMsg::ClaimDistribution {
                to: Addr::unchecked("destination"),
                memo: Some(String::from("note")),
            },
        )
        .unwrap();

        // verify send message
        assert_eq!(1, res.messages.len());
        let (to_address, coins) = send_args(msg_at_index(&res, 0));
        assert_eq!("destination", to_address);
        assert_eq!(10_000, coins.first().unwrap().amount.u128());

        // verify memo
        assert_eq!(1, res.attributes.len());
        let attribute = res.attributes.get(0).unwrap();
        assert_eq!("memo", attribute.key);
        assert_eq!("note", attribute.value);

        // verify distribution is removed
        assert_eq!(
            1,
            outstanding_distributions(&mut deps.storage)
                .load()
                .unwrap()
                .len()
        )
    }

    #[test]
    fn claim_distribution_not_available_yet() {
        let mut deps = default_deps(None);
        outstanding_distributions(&mut deps.storage)
            .save(
                &vec![Distribution {
                    subscription: Addr::unchecked("sub_1"),
                    amount: 10_000,
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
            mock_info("sub_1", &vec![]),
            HandleMsg::ClaimDistribution {
                to: Addr::unchecked("destination"),
                memo: Some(String::from("note")),
            },
        );

        assert!(res.is_err())
    }
}
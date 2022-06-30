use cosmwasm_std::{coins, Addr, BankMsg, DepsMut, Env, MessageInfo, Response};
use provwasm_std::ProvenanceQuery;

use crate::{
    contract::ContractResponse,
    error::contract_error,
    msg::Distribution,
    state::{config_read, outstanding_distributions},
};

pub fn try_issue_distributions(
    deps: DepsMut<ProvenanceQuery>,
    info: MessageInfo,
    mut distributions: Vec<Distribution>,
) -> ContractResponse {
    let state = config_read(deps.storage).load()?;

    if info.sender != state.gp {
        return contract_error("only gp can issue distributions");
    }

    if distributions
        .iter()
        .any(|distribution| !state.accepted_subs.contains(&distribution.subscription))
    {
        return contract_error("subscription not accepted");
    }

    if let Some(mut existing) = outstanding_distributions(deps.storage).may_load()? {
        distributions.append(&mut existing)
    }

    outstanding_distributions(deps.storage).save(&distributions)?;

    Ok(Response::default())
}

pub fn try_cancel_distributions(
    deps: DepsMut<ProvenanceQuery>,
    info: MessageInfo,
    distributions: Vec<Distribution>,
) -> ContractResponse {
    let state = config_read(deps.storage).load()?;

    if info.sender != state.gp {
        return contract_error("only gp can cancel distributions");
    }

    if let Some(mut existing) = outstanding_distributions(deps.storage).may_load()? {
        for distribution in distributions {
            if let Some(index) = existing.iter().position(|it| {
                it.subscription == distribution.subscription && it.amount == distribution.amount
            }) {
                existing.remove(index)
            } else {
                return contract_error("no distribution found");
            };
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
    amount: u64,
    to: Addr,
    memo: Option<String>,
) -> ContractResponse {
    let state = config_read(deps.storage).load()?;

    let mut distributions = outstanding_distributions(deps.storage).load()?;
    let distribution = if let Some(index) = distributions
        .iter()
        .position(|it| it.subscription == info.sender && it.amount == amount)
    {
        distributions.remove(index)
    } else {
        return contract_error("no distribution for subscription");
    };

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
    use cosmwasm_std::testing::{mock_env, mock_info};
    use cosmwasm_std::Addr;
    use cosmwasm_std::Timestamp;

    #[test]
    fn issue_distributions() {
        let mut deps = default_deps(Some(|state| {
            state.accepted_subs.insert(Addr::unchecked("sub_2"));
        }));
        outstanding_distributions(&mut deps.storage)
            .save(&vec![Distribution {
                subscription: Addr::unchecked("sub_1"),
                amount: 10_000,
                available_epoch_seconds: None,
            }])
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
                distributions: vec![],
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
                }],
            },
        );

        assert!(res.is_err());
    }

    #[test]
    fn cancel_distributions() {
        let mut deps = default_deps(None);
        outstanding_distributions(&mut deps.storage)
            .save(&vec![Distribution {
                subscription: Addr::unchecked("sub_1"),
                amount: 10_000,
                available_epoch_seconds: None,
            }])
            .unwrap();

        execute(
            deps.as_mut(),
            mock_env(),
            mock_info("gp", &vec![]),
            HandleMsg::CancelDistributions {
                distributions: vec![Distribution {
                    subscription: Addr::unchecked("sub_1"),
                    amount: 10_000,
                    available_epoch_seconds: None,
                }]
                .into_iter()
                .collect(),
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
                distributions: vec![],
            },
        );

        assert!(res.is_err());
    }

    #[test]
    fn claim_distribution() {
        let mut deps = default_deps(None);
        outstanding_distributions(&mut deps.storage)
            .save(&vec![
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
            ])
            .unwrap();

        let res = execute(
            deps.as_mut(),
            mock_env(),
            mock_info("sub_1", &vec![]),
            HandleMsg::ClaimDistribution {
                amount: 10_000,
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
            .save(&vec![Distribution {
                subscription: Addr::unchecked("sub_1"),
                amount: 10_000,
                available_epoch_seconds: Some(1675209600), // Feb 01 2023 UTC
            }])
            .unwrap();
        let mut env = mock_env();
        env.block.time = Timestamp::from_seconds(1672531200); // Jan 01 2023 UTC

        let res = execute(
            deps.as_mut(),
            mock_env(),
            mock_info("sub_1", &vec![]),
            HandleMsg::ClaimDistribution {
                amount: 10_000,
                to: Addr::unchecked("destination"),
                memo: Some(String::from("note")),
            },
        );

        assert!(res.is_err())
    }
}

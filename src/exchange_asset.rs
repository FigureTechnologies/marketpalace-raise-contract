use std::cmp::Ordering;

use cosmwasm_std::{coins, Addr, BankMsg, DepsMut, Env, MessageInfo, Response};
use provwasm_std::{
    burn_marker_supply, mint_marker_supply, withdraw_coins, ProvenanceQuerier, ProvenanceQuery,
};

use crate::{
    contract::ContractResponse,
    error::contract_error,
    msg::{AssetExchange, ExchangeDate, IssueAssetExchange},
    state::{accepted_subscriptions_read, asset_exchange_storage, config_read},
};

pub fn try_issue_asset_exchanges(
    deps: DepsMut<ProvenanceQuery>,
    info: MessageInfo,
    asset_exchanges: Vec<IssueAssetExchange>,
) -> ContractResponse {
    let state = config_read(deps.storage).load()?;
    let accepted = accepted_subscriptions_read(deps.storage)
        .may_load()?
        .unwrap_or_default();
    let mut storage = asset_exchange_storage(deps.storage);

    if info.sender != state.gp {
        return contract_error("only gp can issue redemptions");
    }

    for mut issuance in asset_exchanges {
        if !accepted.contains(&issuance.subscription) {
            return contract_error("subscription not accepted");
        }

        let mut existing = storage
            .may_load(issuance.subscription.as_bytes())?
            .unwrap_or_default();

        existing.append(&mut issuance.exchanges);

        storage.save(issuance.subscription.as_bytes(), &existing)?;
    }

    Ok(Response::default())
}

pub fn try_cancel_asset_exchanges(
    deps: DepsMut<ProvenanceQuery>,
    info: MessageInfo,
    cancellations: Vec<IssueAssetExchange>,
) -> ContractResponse {
    let state = config_read(deps.storage).load()?;
    let mut storage = asset_exchange_storage(deps.storage);

    if info.sender != state.gp {
        return contract_error("only gp can cancel redemptions");
    }

    for cancel in &cancellations {
        let mut existing = storage
            .may_load(cancel.subscription.as_bytes())?
            .ok_or("no asset exchange found for subscription")?;

        for exchange in &cancel.exchanges {
            let index = existing
                .iter()
                .position(|e| exchange == e)
                .ok_or("no asset exchange found for subcription")?;

            existing.remove(index);
        }

        storage.save(cancel.subscription.as_bytes(), &existing)?;
    }

    Ok(Response::default())
}

pub fn try_complete_asset_exchange(
    deps: DepsMut<ProvenanceQuery>,
    env: Env,
    info: MessageInfo,
    exchanges: Vec<AssetExchange>,
    to: Option<Addr>,
    memo: Option<String>,
) -> ContractResponse {
    let state = config_read(deps.storage).load()?;
    let mut storage = asset_exchange_storage(deps.storage);

    let mut existing = storage
        .may_load(info.sender.as_bytes())?
        .ok_or("no asset exchange found for subscription")?;

    for exchange in &exchanges {
        let index = existing
            .iter()
            .position(|e| exchange == e)
            .ok_or("no asset exchange found for subcription")?;
        existing.remove(index);

        if let Some(date) = &exchange.date {
            match date {
                ExchangeDate::Due(epoch_seconds) => {
                    if epoch_seconds < &env.block.time.seconds() {
                        return contract_error("exchange past due");
                    }
                }
                ExchangeDate::Available(epoch_seconds) => {
                    if epoch_seconds > &env.block.time.seconds() {
                        return contract_error("exchange not yet available");
                    }
                }
            }
        }
    }
    storage.save(info.sender.as_bytes(), &existing)?;

    let mut response = Response::new();

    let total_investment: i64 = exchanges.iter().filter_map(|e| e.investment).sum();
    let abs_investment = total_investment.unsigned_abs();
    match total_investment.cmp(&0) {
        Ordering::Less => {
            let investment_marker = ProvenanceQuerier::new(&deps.querier)
                .get_marker_by_denom(state.investment_denom.clone())?;
            let deposit_investment = BankMsg::Send {
                to_address: investment_marker.address.into_string(),
                amount: coins(abs_investment.into(), state.investment_denom.clone()),
            };
            let burn_investment =
                burn_marker_supply(abs_investment.into(), state.investment_denom.clone())?;

            response = response
                .add_message(deposit_investment)
                .add_message(burn_investment);
        }
        Ordering::Greater => {
            let mint_investment =
                mint_marker_supply(abs_investment.into(), state.investment_denom.clone())?;
            let withdraw_investment = withdraw_coins(
                state.investment_denom.clone(),
                abs_investment.into(),
                state.investment_denom.clone(),
                info.sender.clone(),
            )?;

            response = response
                .add_message(mint_investment)
                .add_message(withdraw_investment);
        }
        _ => {}
    }

    let total_commitment: i64 = exchanges
        .iter()
        .filter_map(|e| e.commitment_in_shares)
        .sum();
    let abs_commitment = total_commitment.unsigned_abs();
    match total_commitment.cmp(&0) {
        Ordering::Less => {
            let deposit_commitment = BankMsg::Send {
                to_address: ProvenanceQuerier::new(&deps.querier)
                    .get_marker_by_denom(state.commitment_denom.clone())?
                    .address
                    .into_string(),
                amount: coins(abs_commitment.into(), state.commitment_denom.clone()),
            };
            let burn_commitment =
                burn_marker_supply(abs_commitment.into(), state.commitment_denom)?;

            response = response
                .add_message(deposit_commitment)
                .add_message(burn_commitment);
        }
        Ordering::Greater => {
            let mint_commitment =
                mint_marker_supply(abs_commitment.into(), state.commitment_denom.clone())?;
            let withdraw_commitment = withdraw_coins(
                state.commitment_denom.clone(),
                abs_commitment.into(),
                state.commitment_denom,
                info.sender.clone(),
            )?;

            response = response
                .add_message(mint_commitment)
                .add_message(withdraw_commitment);
        }
        _ => {}
    }

    let total_capital: i64 = exchanges.iter().filter_map(|e| e.capital).sum();
    let abs_capital = total_capital.unsigned_abs();
    if total_capital > 0 {
        let send_capital = BankMsg::Send {
            to_address: to.unwrap_or(info.sender).into_string(),
            amount: coins(abs_capital.into(), state.capital_denom),
        };

        response = response.add_message(send_capital);
    }

    Ok(match memo {
        Some(memo) => response.add_attribute(String::from("memo"), memo),
        None => response,
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
    use crate::msg::IssueAssetExchange;
    use crate::state::tests::asset_exchange_storage_read;
    use crate::state::tests::set_accepted;
    use cosmwasm_std::from_binary;
    use cosmwasm_std::testing::{mock_env, mock_info};
    use cosmwasm_std::to_binary;
    use cosmwasm_std::Addr;
    use cosmwasm_std::Timestamp;

    #[test]
    fn size() {
        let exchange = AssetExchange {
            investment: Some(-1_000),
            commitment_in_shares: None,
            capital: Some(1_000),
            date: Some(ExchangeDate::Available(0)),
        };
        let as_bytes = to_binary(&exchange).unwrap();
        println!("{}", std::str::from_utf8(as_bytes.as_slice()).unwrap());
        assert_eq!(41, as_bytes.len());

        println!("{:?}", from_binary::<AssetExchange>(&as_bytes).unwrap());
    }

    #[test]
    fn issue_asset_exchange_for_capital_call() {
        let mut deps = default_deps(None);
        set_accepted(&mut deps.storage, vec!["sub_1"]);
        {
            asset_exchange_storage(&mut deps.storage)
                .save(
                    Addr::unchecked("sub_1").as_bytes(),
                    &vec![AssetExchange {
                        investment: None,
                        commitment_in_shares: Some(1_000),
                        capital: None,
                        date: None,
                    }],
                )
                .unwrap();
        }

        execute(
            deps.as_mut(),
            mock_env(),
            mock_info("gp", &vec![]),
            HandleMsg::IssueAssetExchanges {
                asset_exchanges: vec![IssueAssetExchange {
                    subscription: Addr::unchecked("sub_1"),
                    exchanges: vec![AssetExchange {
                        investment: Some(1_000),
                        commitment_in_shares: Some(-1_000),
                        capital: Some(-1_000),
                        date: None,
                    }],
                }],
            },
        )
        .unwrap();

        // verify asset exchange added
        assert_eq!(
            2,
            asset_exchange_storage_read(&deps.storage)
                .load(Addr::unchecked("sub_1").as_bytes())
                .unwrap()
                .len()
        )
    }

    #[test]
    fn issue_asset_exchange_bad_actor() {
        let res = execute(
            default_deps(None).as_mut(),
            mock_env(),
            mock_info("bad_actor", &vec![]),
            HandleMsg::IssueAssetExchanges {
                asset_exchanges: vec![],
            },
        );

        assert!(res.is_err());
    }

    #[test]
    fn issue_asset_exchange_not_accepted() {
        let res = execute(
            default_deps(None).as_mut(),
            mock_env(),
            mock_info("gp", &vec![]),
            HandleMsg::IssueAssetExchanges {
                asset_exchanges: vec![IssueAssetExchange {
                    subscription: Addr::unchecked("sub_1"),
                    exchanges: vec![AssetExchange {
                        investment: Some(1_000),
                        commitment_in_shares: Some(-1_000),
                        capital: Some(-1_000),
                        date: None,
                    }],
                }],
            },
        );

        assert!(res.is_err());
    }

    #[test]
    fn cancel_asset_exchange() {
        let mut deps = default_deps(None);
        {
            asset_exchange_storage(&mut deps.storage)
                .save(
                    Addr::unchecked("sub_1").as_bytes(),
                    &vec![AssetExchange {
                        investment: Some(1_000),
                        commitment_in_shares: Some(-1_000),
                        capital: Some(-1_000),
                        date: None,
                    }],
                )
                .unwrap();
        }

        execute(
            deps.as_mut(),
            mock_env(),
            mock_info("gp", &vec![]),
            HandleMsg::CancelAssetExchanges {
                cancellations: vec![IssueAssetExchange {
                    subscription: Addr::unchecked("sub_1"),
                    exchanges: vec![AssetExchange {
                        investment: Some(1_000),
                        commitment_in_shares: Some(-1_000),
                        capital: Some(-1_000),
                        date: None,
                    }],
                }],
            },
        )
        .unwrap();

        // verify exchange is removed
        assert_eq!(
            0,
            asset_exchange_storage_read(&deps.storage)
                .load(Addr::unchecked("sub_1").as_bytes())
                .unwrap()
                .len()
        )
    }

    #[test]
    fn cancel_asset_exchange_bad_actor() {
        let res = execute(
            default_deps(None).as_mut(),
            mock_env(),
            mock_info("bad_actor", &vec![]),
            HandleMsg::CancelAssetExchanges {
                cancellations: vec![],
            },
        );

        assert!(res.is_err());
    }

    #[test]
    fn cancel_asset_exchange_not_found() {
        let res = execute(
            default_deps(None).as_mut(),
            mock_env(),
            mock_info("gp", &coins(10_000, "stable_coin")),
            HandleMsg::CancelAssetExchanges {
                cancellations: vec![IssueAssetExchange {
                    subscription: Addr::unchecked("sub_1"),
                    exchanges: vec![AssetExchange {
                        investment: Some(1_000),
                        commitment_in_shares: Some(-1_000),
                        capital: Some(-1_000),
                        date: None,
                    }],
                }],
            },
        );

        assert!(res.is_err());
    }

    #[test]
    fn complete_asset_exchange() {
        let mut deps = default_deps(None);
        load_markers(&mut deps.querier);
        {
            asset_exchange_storage(&mut deps.storage)
                .save(
                    Addr::unchecked("sub_1").as_bytes(),
                    &vec![
                        AssetExchange {
                            investment: Some(-1_000),
                            commitment_in_shares: None,
                            capital: Some(1_000),
                            date: None,
                        },
                        AssetExchange {
                            investment: Some(-1_000),
                            commitment_in_shares: None,
                            capital: Some(1_000),
                            date: None,
                        },
                    ],
                )
                .unwrap();
        }

        let res = execute(
            deps.as_mut(),
            mock_env(),
            mock_info("sub_1", &coins(1_000, "investment_coin")),
            HandleMsg::CompleteAssetExchange {
                exchanges: vec![
                    AssetExchange {
                        investment: Some(-1_000),
                        commitment_in_shares: None,
                        capital: Some(1_000),
                        date: None,
                    },
                    AssetExchange {
                        investment: Some(-1_000),
                        commitment_in_shares: None,
                        capital: Some(1_000),
                        date: None,
                    },
                ],
                to: Some(Addr::unchecked("destination")),
                memo: Some(String::from("note")),
            },
        )
        .unwrap();

        assert_eq!(3, res.messages.len());

        // verify memo
        assert_eq!(1, res.attributes.len());
        let attribute = res.attributes.get(0).unwrap();
        assert_eq!("memo", attribute.key);
        assert_eq!("note", attribute.value);

        // verify deposit investment
        let (to_address, coins) = send_args(msg_at_index(&res, 0));
        let coin = coins.first().unwrap();
        assert_eq!("tp18vd8fpwxzck93qlwghaj6arh4p7c5n89x8kskz", to_address);
        assert_eq!("investment_coin", coin.denom);
        assert_eq!(2_000, coin.amount.u128());

        // verify burn investment
        let coin = burn_args(msg_at_index(&res, 1));
        assert_eq!("investment_coin", coin.denom);
        assert_eq!(2_000, coin.amount.u128());

        // verify send message
        let (to_address, coins) = send_args(msg_at_index(&res, 2));
        let coin = coins.first().unwrap();
        assert_eq!("destination", to_address);
        assert_eq!("stable_coin", coin.denom);
        assert_eq!(2_000, coin.amount.u128());

        // verify exchange is removed
        assert_eq!(
            0,
            asset_exchange_storage_read(&deps.storage)
                .load(Addr::unchecked("sub_1").as_bytes())
                .unwrap()
                .len()
        )
    }

    #[test]
    fn complete_asset_exchange_without_asset() {
        let mut deps = default_deps(None);
        {
            asset_exchange_storage(&mut deps.storage)
                .save(
                    Addr::unchecked("sub_1").as_bytes(),
                    &vec![AssetExchange {
                        investment: Some(-1_000),
                        commitment_in_shares: None,
                        capital: Some(1_000),
                        date: None,
                    }],
                )
                .unwrap();
        }

        let res = execute(
            deps.as_mut(),
            mock_env(),
            mock_info("sub_1", &vec![]),
            HandleMsg::CompleteAssetExchange {
                exchanges: vec![AssetExchange {
                    investment: Some(-1_000),
                    commitment_in_shares: None,
                    capital: Some(1_000),
                    date: None,
                }],
                to: Some(Addr::unchecked("destination")),
                memo: Some(String::from("note")),
            },
        );

        assert!(res.is_err());
    }

    #[test]
    fn claim_asset_exchange_not_available_yet() {
        let mut deps = default_deps(None);
        load_markers(&mut deps.querier);
        {
            asset_exchange_storage(&mut deps.storage)
                .save(
                    Addr::unchecked("sub_1").as_bytes(),
                    &vec![AssetExchange {
                        investment: Some(-1_000),
                        commitment_in_shares: None,
                        capital: Some(1_000),
                        date: Some(ExchangeDate::Available(1675209600)), // Feb 01 2023 UTC
                    }],
                )
                .unwrap();
        }
        let mut env = mock_env();
        env.block.time = Timestamp::from_seconds(1672531200); // Jan 01 2023 UTC

        let res = execute(
            deps.as_mut(),
            mock_env(),
            mock_info("sub_1", &coins(1_000, "investment_coin")),
            HandleMsg::CompleteAssetExchange {
                exchanges: vec![AssetExchange {
                    investment: Some(-1_000),
                    commitment_in_shares: None,
                    capital: Some(1_000),
                    date: None,
                }],
                to: Some(Addr::unchecked("destination")),
                memo: Some(String::from("note")),
            },
        );

        assert!(res.is_err());
    }
}

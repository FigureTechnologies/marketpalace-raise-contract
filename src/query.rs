use cosmwasm_std::{entry_point, to_binary, Addr, Binary, Deps, Env, StdResult};
use provwasm_std::ProvenanceQuery;
use schemars::JsonSchema;
use serde::Serialize;

use crate::msg::{AssetExchange, QueryMsg, RaiseState};
use crate::state::{
    accepted_subscriptions_read, asset_exchange_storage_read, config_read,
    eligible_subscriptions_read, pending_subscriptions_read,
};

#[entry_point]
pub fn query(deps: Deps<ProvenanceQuery>, _env: Env, msg: QueryMsg) -> StdResult<Binary> {
    match msg {
        QueryMsg::GetState {} => to_binary(&RaiseState {
            general: config_read(deps.storage).load()?,
            pending_subscriptions: pending_subscriptions_read(deps.storage)
                .may_load()?
                .unwrap_or_default(),
            eligible_subscriptions: eligible_subscriptions_read(deps.storage)
                .may_load()?
                .unwrap_or_default(),
            accepted_subscriptions: accepted_subscriptions_read(deps.storage)
                .may_load()?
                .unwrap_or_default(),
        }),
        QueryMsg::GetAllAssetExchanges {} => {
            let all_asset_exchanges: Vec<SubscriptionAssetExchanges> =
                accepted_subscriptions_read(deps.storage)
                    .may_load()?
                    .unwrap_or_default()
                    .into_iter()
                    .map(|subscription| SubscriptionAssetExchanges {
                        subscription: subscription.clone(),
                        exchanges: asset_exchange_storage_read(deps.storage)
                            .may_load(subscription.as_bytes())
                            .unwrap()
                            .unwrap_or_default(),
                    })
                    .collect();

            to_binary(&all_asset_exchanges)
        }
        QueryMsg::GetAssetExchangesForSubscription { subscription } => {
            to_binary(&asset_exchange_storage_read(deps.storage).may_load(subscription.as_bytes())?)
        }
    }
}

#[derive(Serialize, Clone, Debug, PartialEq, Eq, JsonSchema)]
struct SubscriptionAssetExchanges {
    #[serde(rename = "sub")]
    subscription: Addr,
    exchanges: Vec<AssetExchange>,
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::{
        query::query,
        state::{asset_exchange_storage, tests::set_accepted},
    };
    use cosmwasm_std::testing::mock_env;
    use provwasm_mocks::mock_dependencies;

    #[test]
    fn get_all_asset_exchanges() {
        let mut deps = mock_dependencies(&[]);
        set_accepted(&mut deps.storage, vec!["sub_1"]);
        {
            asset_exchange_storage(&mut deps.storage)
                .save(
                    Addr::unchecked("sub_1").as_bytes(),
                    &vec![AssetExchange {
                        investment: None,
                        commitment_in_shares: Some(1_000),
                        capital_denom: String::from("stable_coin"),
                        capital: None,
                        date: None,
                    }],
                )
                .unwrap();
        }

        let res = query(deps.as_ref(), mock_env(), QueryMsg::GetAllAssetExchanges {}).unwrap();
        println!("{}", std::str::from_utf8(res.as_slice()).unwrap());
    }

    #[test]
    fn get_asset_exchanges_for_subscription() {
        let mut deps = mock_dependencies(&[]);
        asset_exchange_storage(&mut deps.storage)
            .save(
                Addr::unchecked("sub_1").as_bytes(),
                &vec![AssetExchange {
                    investment: None,
                    commitment_in_shares: Some(1_000),
                    capital_denom: String::from("stable_coin"),
                    capital: None,
                    date: None,
                }],
            )
            .unwrap();

        let res = query(deps.as_ref(), mock_env(), QueryMsg::GetAllAssetExchanges {}).unwrap();
        println!("{}", std::str::from_utf8(res.as_slice()).unwrap());
    }
}

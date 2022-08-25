use std::collections::HashMap;

use cosmwasm_std::{entry_point, to_binary, Addr, Binary, Deps, Env, StdResult};
use provwasm_std::ProvenanceQuery;

use crate::msg::{AssetExchange, QueryMsg, RaiseState};
use crate::state::{
    accepted_subscriptions_read, asset_exchange_storage_read, config_read,
    pending_subscriptions_read,
};

#[entry_point]
pub fn query(deps: Deps<ProvenanceQuery>, _env: Env, msg: QueryMsg) -> StdResult<Binary> {
    match msg {
        QueryMsg::GetState {} => to_binary(&RaiseState {
            general: config_read(deps.storage).load()?,
            pending_subscriptions: pending_subscriptions_read(deps.storage)
                .may_load()?
                .unwrap_or_default(),
            accepted_subscriptions: accepted_subscriptions_read(deps.storage)
                .may_load()?
                .unwrap_or_default(),
        }),
        QueryMsg::GetAllAssetExchanges {} => {
            let all_asset_exchanges: HashMap<Addr, Option<Vec<AssetExchange>>> =
                accepted_subscriptions_read(deps.storage)
                    .may_load()?
                    .unwrap_or_default()
                    .into_iter()
                    .map(|subscription| {
                        (
                            subscription.clone(),
                            asset_exchange_storage_read(deps.storage)
                                .may_load(subscription.as_bytes())
                                .unwrap(),
                        )
                    })
                    .collect();

            to_binary(&all_asset_exchanges)
        }
        QueryMsg::GetAssetExchangesForSubscription { subscription } => {
            to_binary(&asset_exchange_storage_read(deps.storage).may_load(subscription.as_bytes())?)
        }
    }
}

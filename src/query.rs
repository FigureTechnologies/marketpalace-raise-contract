use cosmwasm_std::{entry_point, to_binary, Binary, Deps, Env, StdResult};
use provwasm_std::ProvenanceQuery;

use crate::msg::{QueryMsg, RaiseState};
use crate::state::{
    accepted_subscriptions_read, closed_subscriptions_read, config_read,
    outstanding_capital_calls_read, outstanding_commitment_updates_read,
    outstanding_distributions_read, outstanding_redemptions_read,
    outstanding_subscription_closures_read, pending_subscriptions_read,
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
            outstanding_subscription_closures: outstanding_subscription_closures_read(deps.storage)
                .may_load()?
                .unwrap_or_default(),
            closed_subscriptions: closed_subscriptions_read(deps.storage)
                .may_load()?
                .unwrap_or_default(),
            outstanding_commitment_updates: outstanding_commitment_updates_read(deps.storage)
                .may_load()?
                .unwrap_or_default(),
            outstanding_capital_calls: outstanding_capital_calls_read(deps.storage)
                .may_load()?
                .unwrap_or_default(),
            outstanding_redemptions: outstanding_redemptions_read(deps.storage)
                .may_load()?
                .unwrap_or_default(),
            outstanding_distributions: outstanding_distributions_read(deps.storage)
                .may_load()?
                .unwrap_or_default(),
        }),
    }
}

use cosmwasm_std::{entry_point, to_binary, Binary, Deps, Env, StdResult};
use provwasm_std::ProvenanceQuery;

use crate::msg::{QueryMsg, RaiseState};
use crate::state::{accepted_subscriptions_read, config_read, pending_subscriptions_read};

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
    }
}

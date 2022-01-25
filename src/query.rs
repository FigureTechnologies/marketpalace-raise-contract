use cosmwasm_std::{entry_point, to_binary, Binary, Deps, Env, StdResult};

use crate::msg::{QueryMsg, Subs, Terms, Transactions};
use crate::state::config_read;

#[entry_point]
pub fn query(deps: Deps, _env: Env, msg: QueryMsg) -> StdResult<Binary> {
    let state = config_read(deps.storage).load()?;

    match msg {
        QueryMsg::GetStatus {} => to_binary(&state.status),
        QueryMsg::GetTerms {} => to_binary(&Terms {
            acceptable_accreditations: state.acceptable_accreditations,
            other_required_tags: state.other_required_tags,
            commitment_denom: state.commitment_denom,
            investment_denom: state.investment_denom,
            capital_denom: state.capital_denom,
            capital_per_share: state.capital_per_share,
            min_commitment: state.min_commitment,
            max_commitment: state.max_commitment,
        }),
        QueryMsg::GetSubs {} => to_binary(&Subs {
            pending_review: state.pending_review_subs,
            accepted: state.accepted_subs,
        }),
        QueryMsg::GetTransactions {} => to_binary(&Transactions {
            withdrawals: state.issued_withdrawals,
        }),
    }
}

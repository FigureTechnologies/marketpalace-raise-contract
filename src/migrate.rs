use crate::contract::ContractResponse;
use crate::error::ContractError;
use crate::msg::MigrateMsg;
use crate::state::config;
use crate::state::config_read;
use crate::version::CONTRACT_NAME;
use crate::version::CONTRACT_VERSION;
use cosmwasm_std::entry_point;
use cosmwasm_std::to_binary;
use cosmwasm_std::DepsMut;
use cosmwasm_std::Env;
use cosmwasm_std::Response;
use cw2::set_contract_version;
use provwasm_std::ProvenanceQuery;
use serde::Serialize;

#[derive(Serialize)]
struct EmptyArgs {}

#[entry_point]
pub fn migrate(deps: DepsMut<ProvenanceQuery>, _: Env, msg: MigrateMsg) -> ContractResponse {
    set_contract_version(deps.storage, CONTRACT_NAME, CONTRACT_VERSION)?;

    let state = config_read(deps.storage).load()?;

    if state.subscription_code_id != msg.subscription_code_id {
        config(deps.storage).update(|mut state| -> Result<_, ContractError> {
            state.subscription_code_id = msg.subscription_code_id;
            Ok(state)
        })?;

        let sub_migrations = state
            .pending_review_subs
            .union(&state.accepted_subs)
            .map(|addr| cosmwasm_std::WasmMsg::Migrate {
                contract_addr: addr.to_string(),
                new_code_id: msg.subscription_code_id,
                msg: to_binary(&EmptyArgs {}).unwrap(),
            });

        Ok(Response::default().add_messages(sub_migrations))
    } else {
        Ok(Response::default())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::contract::tests::default_deps;
    use crate::state::Status;
    use crate::state::Withdrawal;
    use cosmwasm_std::testing::mock_env;
    use cosmwasm_std::Addr;
    use schemars::JsonSchema;
    use serde::{Deserialize, Serialize};
    use std::collections::HashSet;

    #[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
    pub struct OldState {
        pub subscription_code_id: u64,
        pub status: Status,
        pub recovery_admin: Addr,
        pub gp: Addr,
        pub target: u64,
        pub acceptable_accreditations: HashSet<String>,
        pub other_required_tags: HashSet<String>,
        pub commitment_denom: String,
        pub investment_denom: String,
        pub capital_denom: String,
        pub capital_per_share: u64,
        pub min_commitment: Option<u64>,
        pub max_commitment: Option<u64>,
        pub sequence: u16,
        pub pending_review_subs: HashSet<Addr>,
        pub accepted_subs: HashSet<Addr>,
        pub issued_withdrawals: HashSet<Withdrawal>,
    }

    #[test]
    fn new_sub_code_migration() {
        let mut deps = default_deps(Some(|state| {
            state.accepted_subs = vec![Addr::unchecked("sub_1")].into_iter().collect();
        }));

        let res = migrate(
            deps.as_mut(),
            mock_env(),
            MigrateMsg {
                subscription_code_id: 1,
            },
        )
        .unwrap();

        assert_eq!(1, res.messages.len());
    }

    #[test]
    fn same_sub_code_migration() {
        let mut deps = default_deps(Some(|state| {
            state.accepted_subs = vec![Addr::unchecked("sub_1")].into_iter().collect();
        }));

        let res = migrate(
            deps.as_mut(),
            mock_env(),
            MigrateMsg {
                subscription_code_id: 0,
            },
        )
        .unwrap();

        assert_eq!(0, res.messages.len());
    }
}

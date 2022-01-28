use crate::contract::ContractResponse;
use crate::msg::MigrateMsg;
use crate::version::CONTRACT_NAME;
use crate::version::CONTRACT_VERSION;
use cosmwasm_std::entry_point;
use cosmwasm_std::DepsMut;
use cosmwasm_std::Env;
use cosmwasm_std::Response;
use cw2::set_contract_version;

#[entry_point]
pub fn migrate(deps: DepsMut, _: Env, _: MigrateMsg) -> ContractResponse {
    set_contract_version(deps.storage, CONTRACT_NAME, CONTRACT_VERSION)?;

    Ok(Response::default())
}

#[cfg(test)]
mod tests {
    use crate::state::State;
    use crate::state::Status;
    use crate::state::Withdrawal;
    use cosmwasm_std::testing::mock_dependencies;
    use cosmwasm_std::Addr;
    use cosmwasm_storage::{singleton, singleton_read};
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

    pub static CONFIG_KEY: &[u8] = b"config";

    #[test]
    fn read_from_old_state() {
        let mut deps = mock_dependencies(&[]);

        let old_state = OldState {
            status: Status::Active,
            subscription_code_id: 0,
            recovery_admin: Addr::unchecked("marketpalace"),
            gp: Addr::unchecked("gp"),
            target: 1_000_000,
            acceptable_accreditations: HashSet::new(),
            other_required_tags: HashSet::new(),
            commitment_denom: String::from("commitment_coin"),
            investment_denom: String::from("investment_coin"),
            capital_denom: String::from("stable_coin"),
            capital_per_share: 100,
            min_commitment: Some(10_000),
            max_commitment: Some(100_000),
            sequence: 0,
            pending_review_subs: HashSet::new(),
            accepted_subs: HashSet::new(),
            issued_withdrawals: HashSet::new(),
        };

        singleton(&mut deps.storage, CONFIG_KEY)
            .save(&old_state)
            .unwrap();

        singleton_read::<State>(&deps.storage, CONFIG_KEY)
            .load()
            .unwrap();
    }
}

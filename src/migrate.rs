use std::collections::HashSet;

use crate::contract::ContractResponse;
use crate::msg::MigrateMsg;
use crate::state::config;
use crate::state::State;
use crate::state::CONFIG_KEY;
use crate::version::CONTRACT_NAME;
use crate::version::CONTRACT_VERSION;
use cosmwasm_std::entry_point;
use cosmwasm_std::Addr;
use cosmwasm_std::DepsMut;
use cosmwasm_std::Env;
use cosmwasm_std::Response;
use cosmwasm_storage::singleton_read;
use cw2::{get_contract_version, set_contract_version};
use provwasm_std::ProvenanceQuery;
use serde::Deserialize;
use serde::Serialize;

#[derive(Serialize)]
struct EmptyArgs {}

#[entry_point]
pub fn migrate(
    deps: DepsMut<ProvenanceQuery>,
    _: Env,
    migrate_msg: MigrateMsg,
) -> ContractResponse {
    let contract_info = get_contract_version(deps.storage)?;

    match contract_info.version.as_str() {
        "2.2.0" => {
            let old_state: State = singleton_read(deps.storage, CONFIG_KEY).load()?;
            let capital_denom = match migrate_msg.capital_denom {
                None => old_state.capital_denom,
                Some(capital_denom) => capital_denom,
            };
            let new_state = State {
                subscription_code_id: migrate_msg.subscription_code_id,
                recovery_admin: old_state.recovery_admin,
                gp: old_state.gp,
                required_attestations: old_state.required_attestations,
                commitment_denom: old_state.commitment_denom,
                investment_denom: old_state.investment_denom,
                capital_denom,
                capital_per_share: old_state.capital_per_share,
                required_capital_attribute: migrate_msg.required_capital_attribute,
            };

            config(deps.storage).save(&new_state)?;
        }
        _ => {
            let old_state: StateV2_0_0 = singleton_read(deps.storage, CONFIG_KEY).load()?;
            let capital_denom = match migrate_msg.capital_denom {
                None => old_state.capital_denom,
                Some(capital_denom) => capital_denom,
            };
            let new_state = State {
                subscription_code_id: migrate_msg.subscription_code_id,
                recovery_admin: old_state.recovery_admin,
                gp: old_state.gp,
                required_attestations: vec![old_state.acceptable_accreditations],
                commitment_denom: old_state.commitment_denom,
                investment_denom: old_state.investment_denom,
                capital_denom,
                capital_per_share: old_state.capital_per_share,
                required_capital_attribute: migrate_msg.required_capital_attribute,
            };

            config(deps.storage).save(&new_state)?;
        }
    };

    set_contract_version(deps.storage, CONTRACT_NAME, CONTRACT_VERSION)?;

    Ok(Response::default())
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct StateV2_0_0 {
    pub subscription_code_id: u64,
    pub recovery_admin: Addr,
    pub gp: Addr,
    pub acceptable_accreditations: HashSet<String>,
    pub commitment_denom: String,
    pub investment_denom: String,
    pub capital_denom: String,
    pub capital_per_share: u64,
}

#[cfg(test)]
mod tests {
    use crate::migrate::{migrate, StateV2_0_0};
    use crate::msg::MigrateMsg;
    use crate::state::{State, CONFIG_KEY};
    use cosmwasm_std::testing::mock_env;
    use cosmwasm_std::Addr;
    use cosmwasm_storage::{singleton, singleton_read};
    use cw2::set_contract_version;
    use provwasm_mocks::mock_dependencies;
    use std::collections::HashSet;

    #[test]
    fn migration() {
        let mut deps = mock_dependencies(&[]);
        set_contract_version(&mut deps.storage, "TEST", "2.0.0").unwrap();
        singleton(&mut deps.storage, CONFIG_KEY)
            .save(&StateV2_0_0 {
                subscription_code_id: 1,
                recovery_admin: Addr::unchecked("marketpalace"),
                gp: Addr::unchecked("gp"),
                acceptable_accreditations: HashSet::from(["506c".to_string()]),
                commitment_denom: "commitment".to_string(),
                investment_denom: "investment".to_string(),
                capital_denom: String::from("stable_coin"),
                capital_per_share: 100,
            })
            .unwrap();

        migrate(
            deps.as_mut(),
            mock_env(),
            MigrateMsg {
                subscription_code_id: 2,
                capital_denom: None,
                required_capital_attribute: None,
            },
        )
        .unwrap();

        assert_eq!(
            State {
                subscription_code_id: 2,
                recovery_admin: Addr::unchecked("marketpalace"),
                required_attestations: vec![HashSet::from(["506c".to_string()])],
                gp: Addr::unchecked("gp"),
                commitment_denom: String::from("commitment"),
                investment_denom: String::from("investment"),
                capital_denom: String::from("stable_coin"),
                capital_per_share: 100,
                required_capital_attribute: None,
            },
            singleton_read(&deps.storage, CONFIG_KEY).load().unwrap()
        );
    }

    #[test]
    fn migration_2_2_0() {
        let mut deps = mock_dependencies(&[]);
        set_contract_version(&mut deps.storage, "TEST", "2.2.0").unwrap();
        singleton(&mut deps.storage, CONFIG_KEY)
            .save(&State {
                subscription_code_id: 1,
                recovery_admin: Addr::unchecked("marketpalace"),
                required_attestations: vec![HashSet::from(["506c".to_string()])],
                gp: Addr::unchecked("gp"),
                commitment_denom: String::from("commitment"),
                investment_denom: String::from("investment"),
                capital_denom: String::from("stable_coin"),
                capital_per_share: 100,
                required_capital_attribute: None,
            })
            .unwrap();

        migrate(
            deps.as_mut(),
            mock_env(),
            MigrateMsg {
                subscription_code_id: 2,
                capital_denom: None,
                required_capital_attribute: None,
            },
        )
        .unwrap();

        assert_eq!(
            State {
                subscription_code_id: 2,
                recovery_admin: Addr::unchecked("marketpalace"),
                required_attestations: vec![HashSet::from(["506c".to_string()])],
                gp: Addr::unchecked("gp"),
                commitment_denom: String::from("commitment"),
                investment_denom: String::from("investment"),
                capital_denom: String::from("stable_coin"),
                capital_per_share: 100,
                required_capital_attribute: None,
            },
            singleton_read(&deps.storage, CONFIG_KEY).load().unwrap()
        );
    }
}

use std::collections::HashSet;

use crate::contract::ContractResponse;
use crate::error::contract_error;
use crate::msg::CapitalDenomRequirement;
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
        "2.3.0" | "2.3.1" => {
            let mut state: State = singleton_read(deps.storage, CONFIG_KEY).load()?;

            state.subscription_code_id = migrate_msg.subscription_code_id;

            config(deps.storage).save(&state)?;
        }
        "2.2.0" | "2.2.1" => {
            let old_state: StateV2_2_0 = singleton_read(deps.storage, CONFIG_KEY).load()?;
            let required_capital_attributes =
                migrate_msg.required_capital_attributes.unwrap_or_else(|| {
                    if let Some(required_attribute) = &old_state.required_capital_attribute {
                        vec![CapitalDenomRequirement {
                            capital_denom: old_state.capital_denom.clone(),
                            required_attribute: required_attribute.clone(),
                        }]
                    } else {
                        vec![]
                    }
                });
            let new_state = State {
                subscription_code_id: migrate_msg.subscription_code_id,
                recovery_admin: old_state.recovery_admin,
                gp: old_state.gp,
                required_attestations: old_state.required_attestations,
                commitment_denom: old_state.commitment_denom,
                investment_denom: old_state.investment_denom,
                like_capital_denoms: migrate_msg
                    .like_capital_denoms
                    .unwrap_or(vec![old_state.capital_denom]),
                capital_per_share: old_state.capital_per_share,
                required_capital_attributes,
            };

            config(deps.storage).save(&new_state)?;
        }
        _ => {
            return contract_error(&format!(
                "existing contract version not supported for migration to {}",
                contract_info.version
            ));
        }
    };

    set_contract_version(deps.storage, CONTRACT_NAME, CONTRACT_VERSION)?;

    Ok(Response::default())
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct StateV2_2_0 {
    pub subscription_code_id: u64,
    pub recovery_admin: Addr,
    pub gp: Addr,
    pub required_attestations: Vec<HashSet<String>>,
    pub commitment_denom: String,
    pub investment_denom: String,
    pub capital_denom: String,
    pub capital_per_share: u64,
    pub required_capital_attribute: Option<String>,
}

#[cfg(test)]
mod tests {
    use crate::migrate::{migrate, StateV2_2_0};
    use crate::msg::MigrateMsg;
    use crate::state::{State, CONFIG_KEY};
    use cosmwasm_std::testing::mock_env;
    use cosmwasm_std::Addr;
    use cosmwasm_storage::{singleton, singleton_read};
    use cw2::set_contract_version;
    use provwasm_mocks::mock_dependencies;
    use std::collections::HashSet;

    #[test]
    fn migration_2_2_0() {
        let mut deps = mock_dependencies(&[]);
        set_contract_version(&mut deps.storage, "TEST", "2.2.0").unwrap();
        singleton(&mut deps.storage, CONFIG_KEY)
            .save(&StateV2_2_0 {
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
                like_capital_denoms: Some(vec![String::from("stable_coin")]),
                required_capital_attributes: None,
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
                like_capital_denoms: vec![String::from("stable_coin")],
                capital_per_share: 100,
                required_capital_attributes: vec![],
            },
            singleton_read(&deps.storage, CONFIG_KEY).load().unwrap()
        );
    }
}

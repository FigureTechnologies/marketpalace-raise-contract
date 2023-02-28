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
use cw2::set_contract_version;
use provwasm_std::ProvenanceQuery;
use serde::Deserialize;
use serde::Serialize;

#[derive(Serialize)]
struct EmptyArgs {}

#[entry_point]
pub fn migrate(deps: DepsMut<ProvenanceQuery>, _: Env, _: MigrateMsg) -> ContractResponse {
    set_contract_version(deps.storage, CONTRACT_NAME, CONTRACT_VERSION)?;

    let old_state: StateV2_0_0 = singleton_read(deps.storage, CONFIG_KEY).load()?;

    let new_state = State {
        subscription_code_id: old_state.subscription_code_id,
        recovery_admin: old_state.recovery_admin,
        gp: old_state.gp,
        required_attestations: vec![old_state.acceptable_accreditations],
        commitment_denom: old_state.commitment_denom,
        investment_denom: old_state.investment_denom,
        capital_denom: old_state.capital_denom,
        capital_per_share: old_state.capital_per_share,
        fiat_deposit_addr: None,
    };

    config(deps.storage).save(&new_state)?;

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

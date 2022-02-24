use crate::contract::ContractResponse;
use crate::error::ContractError;
use crate::msg::MigrateMsg;
use crate::state::config;
use crate::state::config_read;
use crate::version::CONTRACT_NAME;
use crate::version::CONTRACT_VERSION;
use cosmwasm_std::entry_point;
use cosmwasm_std::to_binary;
use cosmwasm_std::Addr;
use cosmwasm_std::DepsMut;
use cosmwasm_std::Env;
use cosmwasm_std::Response;
use cw2::set_contract_version;
use std::collections::HashSet;

#[entry_point]
pub fn migrate(deps: DepsMut, _: Env, msg: MigrateMsg) -> ContractResponse {
    set_contract_version(deps.storage, CONTRACT_NAME, CONTRACT_VERSION)?;

    let state = config_read(deps.storage).load()?;
    let existing_subs: HashSet<&Addr> = state
        .pending_review_subs
        .union(&state.accepted_subs)
        .collect();
    let sub_migrations = existing_subs
        .iter()
        .map(|addr| cosmwasm_std::WasmMsg::Migrate {
            contract_addr: addr.to_string(),
            new_code_id: msg.subscription_code_id,
            msg: to_binary("{}").unwrap(),
        });

    config(deps.storage).update(|mut state| -> Result<_, ContractError> {
        state.subscription_code_id = msg.subscription_code_id;
        Ok(state)
    })?;

    Ok(Response::default().add_messages(sub_migrations))
}

use crate::contract::ContractResponse;
use crate::msg::MigrateMsg;
use crate::version::CONTRACT_NAME;
use crate::version::CONTRACT_VERSION;
use cosmwasm_std::entry_point;
use cosmwasm_std::DepsMut;
use cosmwasm_std::Env;
use cosmwasm_std::Response;
use cw2::set_contract_version;
use provwasm_std::ProvenanceQuery;
use serde::Serialize;

#[derive(Serialize)]
struct EmptyArgs {}

#[entry_point]
pub fn migrate(deps: DepsMut<ProvenanceQuery>, _: Env, _: MigrateMsg) -> ContractResponse {
    set_contract_version(deps.storage, CONTRACT_NAME, CONTRACT_VERSION)?;

    Ok(Response::default())
}

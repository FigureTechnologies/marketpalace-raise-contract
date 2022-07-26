use serde::{Deserialize, Serialize};

use cosmwasm_std::Addr;

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "snake_case")]
pub struct SubInstantiateMsg {
    pub recovery_admin: Addr,
    pub lp: Addr,
    pub commitment_denom: String,
    pub investment_denom: String,
    pub capital_denom: String,
    pub capital_per_share: u64,
}

#[derive(Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SubQueryMsg {
    GetState {},
}

#[derive(Deserialize, Serialize)]
pub struct SubState {
    pub recovery_admin: Addr,
    pub lp: Addr,
    pub raise: Addr,
    pub commitment_denom: String,
    pub investment_denom: String,
    pub capital_denom: String,
    pub capital_per_share: u64,
}

use serde::{Deserialize, Serialize};

use cosmwasm_std::Addr;

use crate::msg::CapitalDenomRequirement;

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "snake_case")]
pub struct SubInstantiateMsg {
    pub admin: Addr,
    pub lp: Addr,
    pub commitment_denom: String,
    pub investment_denom: String,
    pub like_capital_denoms: Vec<String>,
    pub capital_per_share: u64,
    pub initial_commitment: Option<u64>,
    pub required_capital_attributes: Vec<CapitalDenomRequirement>,
}

#[derive(Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SubQueryMsg {
    GetState {},
}

#[derive(Deserialize, Serialize)]
pub struct SubState {
    pub admin: Addr,
    pub lp: Addr,
    pub raise: Addr,
    pub commitment_denom: String,
    pub investment_denom: String,
    pub like_capital_denoms: Vec<String>,
    pub capital_per_share: u64,
    pub required_capital_attributes: Vec<CapitalDenomRequirement>,
}

use serde::{Deserialize, Serialize};

use cosmwasm_std::Addr;

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "snake_case")]
pub struct SubInstantiateMsg {
    pub lp: Addr,
    pub admin: Addr,
    pub capital_denom: String,
    pub min_commitment: u64,
    pub max_commitment: u64,
    pub min_days_of_notice: Option<u16>,
}

#[derive(Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SubExecuteMsg {
    Accept {},
    IssueCapitalCall { capital_call: SubCapitalCall },
    CloseCapitalCall {},
    IssueRedemption { redemption: u64 },
    IssueDistribution {},
}

#[derive(Serialize, Deserialize)]
pub struct SubCapitalCall {
    pub amount: u64,
    pub days_of_notice: Option<u16>,
}

#[derive(Deserialize, Serialize)]
pub struct SubTerms {
    pub owner: Addr,
    pub raise: Addr,
    pub capital_denom: String,
    pub min_commitment: u64,
    pub max_commitment: u64,
}

#[derive(Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SubQueryMsg {
    GetTerms {},
}

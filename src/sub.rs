use serde::{Deserialize, Serialize};

use cosmwasm_std::Addr;

#[derive(Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SubExecuteMsg {
    Accept { commitment: u64 },
    IssueCapitalCall { capital_call: Addr },
    IssueDistribution {},
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

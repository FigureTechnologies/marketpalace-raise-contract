use serde::{Deserialize, Serialize};
use std::collections::HashSet;

use cosmwasm_std::Addr;

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "snake_case")]
pub struct SubInstantiateMsg {
    pub recovery_admin: Addr,
    pub lp: Addr,
    pub capital_denom: String,
    pub min_commitment: u64,
    pub max_commitment: u64,
    pub min_days_of_notice: Option<u16>,
}

#[derive(Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SubExecuteMsg {
    Accept {},
    IssueCapitalCall {
        capital_call: SubCapitalCallIssuance,
    },
    CloseCapitalCall {},
    IssueRedemption {
        redemption: u64,
    },
    IssueDistribution {},
}

#[derive(Serialize, Deserialize)]
pub struct SubCapitalCallIssuance {
    pub amount: u64,
    pub days_of_notice: Option<u16>,
}

#[derive(Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SubQueryMsg {
    GetTerms {},
    GetTransactions {},
}

#[derive(Deserialize, Serialize)]
pub struct SubTerms {
    pub lp: Addr,
    pub raise: Addr,
    pub capital_denom: String,
    pub min_commitment: u64,
    pub max_commitment: u64,
}

#[derive(Deserialize, Serialize)]
pub struct SubTransactions {
    pub capital_calls: SubCapitalCalls,
    pub redemptions: HashSet<SubRedemption>,
    pub distributions: HashSet<SubDistribution>,
    pub withdrawals: HashSet<SubWithdrawal>,
}

#[derive(Deserialize, Serialize)]
pub struct SubCapitalCalls {
    pub active: Option<SubCapitalCall>,
    pub closed: HashSet<SubCapitalCall>,
    pub cancelled: HashSet<SubCapitalCall>,
}

#[derive(Deserialize, Serialize, PartialEq, Eq, Hash)]
pub struct SubCapitalCall {
    pub sequence: u16,
    pub amount: u64,
    pub days_of_notice: Option<u16>,
}

#[derive(Deserialize, Serialize, PartialEq, Eq, Hash)]
pub struct SubRedemption {
    pub sequence: u16,
    pub asset: u64,
    pub capital: u64,
}

#[derive(Deserialize, Serialize, PartialEq, Eq, Hash)]
pub struct SubDistribution {
    pub sequence: u16,
    pub amount: u64,
}

#[derive(Deserialize, Serialize, PartialEq, Eq, Hash)]
pub struct SubWithdrawal {
    pub sequence: u16,
    pub amount: u64,
}

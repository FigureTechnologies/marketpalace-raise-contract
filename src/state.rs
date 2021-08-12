use crate::msg::Call;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

use cosmwasm_std::{Addr, Storage};
use cosmwasm_storage::{singleton, singleton_read, ReadonlySingleton, Singleton};

pub static CONFIG_KEY: &[u8] = b"config";

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct State {
    pub subscription_code_id: u64,
    pub status: Status,
    pub gp: Addr,
    pub admin: Addr,
    pub acceptable_accreditations: HashSet<String>,
    pub other_required_tags: HashSet<String>,
    pub asset_denom: String,
    pub capital_denom: String,
    pub target: u64,
    pub min_commitment: Option<u64>,
    pub max_commitment: Option<u64>,
    pub pending_review_subs: HashSet<Addr>,
    pub accepted_subs: HashSet<Addr>,
    pub issued_calls: HashSet<Call>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub enum Status {
    Active,
}

pub fn config(storage: &mut dyn Storage) -> Singleton<State> {
    singleton(storage, CONFIG_KEY)
}

pub fn config_read(storage: &dyn Storage) -> ReadonlySingleton<State> {
    singleton_read(storage, CONFIG_KEY)
}

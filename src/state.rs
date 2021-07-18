use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

use cosmwasm_std::{Addr, Storage};
use cosmwasm_storage::{singleton, singleton_read, ReadonlySingleton, Singleton};

pub static CONFIG_KEY: &[u8] = b"config";

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct State {
    pub status: Status,
    pub capital_call_code_id: u64,
    pub gp: Addr,
    pub admin: Addr,
    pub qualified_tags: Vec<String>,
    pub asset_denom: String,
    pub capital_denom: String,
    pub target: u64,
    pub min_commitment: u64,
    pub max_commitment: u64,
    pub pending_review_subs: HashSet<Addr>,
    pub accepted_subs: HashSet<Addr>,
    pub issued_calls: HashSet<Addr>,
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

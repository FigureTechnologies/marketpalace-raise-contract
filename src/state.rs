use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::hash::Hash;

use cosmwasm_std::{Addr, Storage};
use cosmwasm_storage::{singleton, singleton_read, ReadonlySingleton, Singleton};

use crate::msg::{Distribution, Redemption};

pub static CONFIG_KEY: &[u8] = b"config";
pub static REDEMPTIONS_KEY: &[u8] = b"redemptions";
pub static DISTRIBUTION_KEY: &[u8] = b"distribution";

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct State {
    pub subscription_code_id: u64,
    pub status: Status,
    pub recovery_admin: Addr,
    pub gp: Addr,
    pub acceptable_accreditations: HashSet<String>,
    pub other_required_tags: HashSet<String>,
    pub commitment_denom: String,
    pub investment_denom: String,
    pub capital_denom: String,
    pub capital_per_share: u64,
    pub min_commitment: Option<u64>,
    pub max_commitment: Option<u64>,
    pub sequence: u16,
    pub pending_review_subs: HashSet<Addr>,
    pub accepted_subs: HashSet<Addr>,
    pub issued_withdrawals: HashSet<Withdrawal>,
}

impl State {
    pub fn not_evenly_divisble(&self, amount: u64) -> bool {
        amount % self.capital_per_share > 0
    }

    pub fn capital_to_shares(&self, amount: u64) -> u64 {
        amount / self.capital_per_share
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub enum Status {
    Active,
}

#[derive(Serialize, Deserialize, Clone, Debug, Eq, JsonSchema)]
pub struct Withdrawal {
    pub sequence: u16,
    pub to: Addr,
    pub amount: u64,
}

impl PartialEq for Withdrawal {
    fn eq(&self, other: &Self) -> bool {
        self.sequence == other.sequence
    }
}

impl Hash for Withdrawal {
    fn hash<H>(&self, state: &mut H)
    where
        H: std::hash::Hasher,
    {
        self.sequence.hash(state);
    }
}

pub fn config(storage: &mut dyn Storage) -> Singleton<State> {
    singleton(storage, CONFIG_KEY)
}

pub fn config_read(storage: &dyn Storage) -> ReadonlySingleton<State> {
    singleton_read(storage, CONFIG_KEY)
}

pub fn outstanding_redemptions(storage: &mut dyn Storage) -> Singleton<Vec<Redemption>> {
    singleton(storage, REDEMPTIONS_KEY)
}

pub fn outstanding_distributions(storage: &mut dyn Storage) -> Singleton<Vec<Distribution>> {
    singleton(storage, DISTRIBUTION_KEY)
}

#[cfg(test)]
mod tests {
    use super::*;

    impl State {
        pub fn test_default() -> State {
            State {
                status: Status::Active,
                subscription_code_id: 0,
                recovery_admin: Addr::unchecked("marketpalace"),
                gp: Addr::unchecked("gp"),
                acceptable_accreditations: HashSet::new(),
                other_required_tags: HashSet::new(),
                commitment_denom: String::from("commitment_coin"),
                investment_denom: String::from("investment_coin"),
                capital_denom: String::from("stable_coin"),
                capital_per_share: 100,
                min_commitment: Some(10_000),
                max_commitment: Some(100_000),
                sequence: 0,
                pending_review_subs: HashSet::new(),
                accepted_subs: HashSet::new(),
                issued_withdrawals: HashSet::new(),
            }
        }
    }

    #[test]
    fn not_evenly_divisble() {
        let state = State::test_default();

        assert_eq!(false, state.not_evenly_divisble(100));
        assert!(state.not_evenly_divisble(101));
        assert_eq!(false, state.not_evenly_divisble(1_000));
        assert!(state.not_evenly_divisble(1_001));
    }
}

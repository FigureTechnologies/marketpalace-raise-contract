use provwasm_std::{ProvenanceQuerier, ProvenanceQuery};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

use cosmwasm_std::{coins, Addr, BankMsg, Deps, StdResult, Storage};
use cosmwasm_storage::{bucket, singleton, singleton_read, Bucket, ReadonlySingleton, Singleton};

use crate::msg::AssetExchange;

pub static CONFIG_KEY: &[u8] = b"config";

pub static ASSET_EXCHANGE_NAMESPACE: &[u8] = b"asset_exchange";

pub static PENDING_SUBSCRIPTIONS_KEY: &[u8] = b"pending_subscriptions";
pub static ACCEPTED_SUBSCRIPTIONS_KEY: &[u8] = b"accepted_subscriptions";

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct State {
    pub subscription_code_id: u64,
    pub recovery_admin: Addr,
    pub gp: Addr,
    pub acceptable_accreditations: HashSet<String>,
    pub other_required_tags: HashSet<String>,
    pub commitment_denom: String,
    pub investment_denom: String,
    pub capital_denom: String,
    pub capital_per_share: u64,
}

impl State {
    pub fn not_evenly_divisble(&self, amount: u64) -> bool {
        amount % self.capital_per_share > 0
    }

    pub fn capital_to_shares(&self, amount: u64) -> u64 {
        amount / self.capital_per_share
    }

    pub fn remaining_commitment(
        &self,
        deps: Deps<ProvenanceQuery>,
        subscription: &Addr,
    ) -> StdResult<u128> {
        deps.querier
            .query_balance(subscription, self.commitment_denom.clone())
            .map(|coin| coin.amount.u128())
    }

    pub fn deposit_commitment_msg(
        &self,
        deps: Deps<ProvenanceQuery>,
        amount: u128,
    ) -> StdResult<BankMsg> {
        Ok(BankMsg::Send {
            to_address: ProvenanceQuerier::new(&deps.querier)
                .get_marker_by_denom(self.commitment_denom.clone())?
                .address
                .into_string(),
            amount: coins(amount, self.commitment_denom.clone()),
        })
    }
}

pub fn config(storage: &mut dyn Storage) -> Singleton<State> {
    singleton(storage, CONFIG_KEY)
}

pub fn config_read(storage: &dyn Storage) -> ReadonlySingleton<State> {
    singleton_read(storage, CONFIG_KEY)
}

pub fn asset_exchange_storage(storage: &mut dyn Storage) -> Bucket<Vec<AssetExchange>> {
    bucket(storage, ASSET_EXCHANGE_NAMESPACE)
}

pub fn pending_subscriptions(storage: &mut dyn Storage) -> Singleton<HashSet<Addr>> {
    singleton(storage, PENDING_SUBSCRIPTIONS_KEY)
}

pub fn pending_subscriptions_read(storage: &dyn Storage) -> ReadonlySingleton<HashSet<Addr>> {
    singleton_read(storage, PENDING_SUBSCRIPTIONS_KEY)
}

pub fn accepted_subscriptions(storage: &mut dyn Storage) -> Singleton<HashSet<Addr>> {
    singleton(storage, ACCEPTED_SUBSCRIPTIONS_KEY)
}

pub fn accepted_subscriptions_read(storage: &dyn Storage) -> ReadonlySingleton<HashSet<Addr>> {
    singleton_read(storage, ACCEPTED_SUBSCRIPTIONS_KEY)
}

#[cfg(test)]
pub mod tests {
    use cosmwasm_storage::{bucket_read, ReadonlyBucket};

    use super::*;

    impl State {
        pub fn test_default() -> State {
            State {
                subscription_code_id: 0,
                recovery_admin: Addr::unchecked("marketpalace"),
                gp: Addr::unchecked("gp"),
                acceptable_accreditations: HashSet::new(),
                other_required_tags: HashSet::new(),
                commitment_denom: String::from("commitment_coin"),
                investment_denom: String::from("investment_coin"),
                capital_denom: String::from("stable_coin"),
                capital_per_share: 100,
            }
        }
    }

    pub fn asset_exchange_storage_read(
        storage: &dyn Storage,
    ) -> ReadonlyBucket<Vec<AssetExchange>> {
        bucket_read(storage, ASSET_EXCHANGE_NAMESPACE)
    }

    pub fn to_addresses(addresses: Vec<&str>) -> HashSet<Addr> {
        addresses
            .into_iter()
            .map(|addr| Addr::unchecked(addr))
            .collect()
    }

    pub fn set_pending(storage: &mut dyn Storage, addresses: Vec<&str>) {
        pending_subscriptions(storage)
            .save(&to_addresses(addresses))
            .unwrap();
    }

    pub fn set_accepted(storage: &mut dyn Storage, addresses: Vec<&str>) {
        accepted_subscriptions(storage)
            .save(&to_addresses(addresses))
            .unwrap();
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

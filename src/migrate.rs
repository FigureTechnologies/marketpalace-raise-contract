use std::collections::HashSet;
use std::hash::Hash;

use crate::contract::ContractResponse;
use crate::msg::CapitalCall;
use crate::msg::MigrateMsg;
use crate::state::accepted_subscriptions;
use crate::state::config;
use crate::state::outstanding_capital_calls;
use crate::state::pending_subscriptions;
use crate::state::State;
use crate::state::CONFIG_KEY;
use crate::version::CONTRACT_NAME;
use crate::version::CONTRACT_VERSION;
use cosmwasm_std::entry_point;
use cosmwasm_std::Addr;
use cosmwasm_std::DepsMut;
use cosmwasm_std::Env;
use cosmwasm_std::Response;
use cosmwasm_storage::singleton_read;
use cw2::set_contract_version;
use provwasm_std::ProvenanceQuery;
use serde::Deserialize;
use serde::Serialize;

#[derive(Serialize)]
struct EmptyArgs {}

#[entry_point]
pub fn migrate(deps: DepsMut<ProvenanceQuery>, _: Env, msg: MigrateMsg) -> ContractResponse {
    set_contract_version(deps.storage, CONTRACT_NAME, CONTRACT_VERSION)?;

    let old_state: StateV1_0_1 = singleton_read(deps.storage, CONFIG_KEY).load()?;

    let new_state = State {
        subscription_code_id: msg.subscription_code_id,
        recovery_admin: old_state.recovery_admin,
        gp: old_state.gp,
        acceptable_accreditations: old_state.acceptable_accreditations,
        other_required_tags: old_state.other_required_tags,
        commitment_denom: old_state.commitment_denom,
        investment_denom: old_state.investment_denom,
        capital_denom: old_state.capital_denom,
        capital_per_share: old_state.capital_per_share,
    };
    let new_pending_subscriptions = old_state.pending_review_subs;
    let new_accepted_subscriptions = old_state.accepted_subs;

    let new_capital_calls = new_accepted_subscriptions
        .iter()
        .filter_map(|subscription| {
            let transactions: Transactions = deps
                .querier
                .query_wasm_smart(subscription.clone(), &QueryMsg::GetTransactions {})
                .unwrap();

            transactions.capital_calls.active.map(|call| CapitalCall {
                subscription: subscription.clone(),
                amount: call.amount,
                due_epoch_seconds: None,
            })
        })
        .collect();

    config(deps.storage).save(&new_state)?;
    pending_subscriptions(deps.storage).save(&new_pending_subscriptions)?;
    accepted_subscriptions(deps.storage).save(&new_accepted_subscriptions)?;
    outstanding_capital_calls(deps.storage).save(&new_capital_calls)?;

    Ok(Response::default())
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
struct StateV1_0_1 {
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

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub enum Status {
    Active,
}

#[derive(Serialize, Deserialize, Clone, Debug, Eq)]
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

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum QueryMsg {
    GetTransactions {},
}

#[derive(Deserialize, Serialize)]
pub struct Transactions {
    pub capital_calls: CapitalCalls,
}

#[derive(Deserialize, Serialize)]
pub struct CapitalCalls {
    pub active: Option<SubCapitalCall>,
}

#[derive(Serialize, Deserialize, Clone, Debug, Eq)]
pub struct SubCapitalCall {
    pub sequence: u16,
    pub amount: u64,
    pub days_of_notice: Option<u16>,
}

impl PartialEq for SubCapitalCall {
    fn eq(&self, other: &Self) -> bool {
        self.sequence == other.sequence
    }
}

impl Hash for SubCapitalCall {
    fn hash<H>(&self, state: &mut H)
    where
        H: std::hash::Hasher,
    {
        self.sequence.hash(state);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mock::wasm_smart_mock_dependencies;
    use crate::state::{
        accepted_subscriptions_read, outstanding_capital_calls_read, pending_subscriptions_read,
    };
    use cosmwasm_std::testing::mock_env;
    use cosmwasm_std::{to_binary, Addr, ContractResult, SystemResult};
    use cosmwasm_storage::{singleton, singleton_read};

    #[test]
    fn migration() {
        let mut deps = wasm_smart_mock_dependencies(&vec![], |_, _| {
            SystemResult::Ok(ContractResult::Ok(
                to_binary(&Transactions {
                    capital_calls: CapitalCalls {
                        active: Some(SubCapitalCall {
                            sequence: 0,
                            amount: 10_000,
                            days_of_notice: None,
                        }),
                    },
                })
                .unwrap(),
            ))
        });
        singleton(&mut deps.storage, CONFIG_KEY)
            .save(&StateV1_0_1 {
                subscription_code_id: 0,
                status: Status::Active,
                recovery_admin: Addr::unchecked("marketpalace"),
                gp: Addr::unchecked("gp"),
                acceptable_accreditations: HashSet::new(),
                other_required_tags: HashSet::new(),
                commitment_denom: String::from("commitment_coin"),
                investment_denom: String::from("investment_coin"),
                capital_denom: String::from("stable_coin"),
                capital_per_share: 100,
                min_commitment: None,
                max_commitment: None,
                sequence: 0,
                pending_review_subs: vec![Addr::unchecked("sub_2")].into_iter().collect(),
                accepted_subs: vec![Addr::unchecked("sub_1")].into_iter().collect(),
                issued_withdrawals: vec![].into_iter().collect(),
            })
            .unwrap();

        migrate(
            deps.as_mut(),
            mock_env(),
            MigrateMsg {
                subscription_code_id: 1,
            },
        )
        .unwrap();

        assert_eq!(
            State {
                subscription_code_id: 1,
                recovery_admin: Addr::unchecked("marketpalace"),
                gp: Addr::unchecked("gp"),
                acceptable_accreditations: HashSet::new(),
                other_required_tags: HashSet::new(),
                commitment_denom: String::from("commitment_coin"),
                investment_denom: String::from("investment_coin"),
                capital_denom: String::from("stable_coin"),
                capital_per_share: 100,
            },
            singleton_read(&deps.storage, CONFIG_KEY).load().unwrap()
        );
        assert_eq!(
            1,
            pending_subscriptions_read(&deps.storage)
                .load()
                .unwrap()
                .len()
        );
        assert_eq!(
            1,
            accepted_subscriptions_read(&deps.storage)
                .load()
                .unwrap()
                .len()
        );
        assert_eq!(
            1,
            outstanding_capital_calls_read(&deps.storage)
                .load()
                .unwrap()
                .len()
        );
    }
}

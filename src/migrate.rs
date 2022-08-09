use std::collections::HashSet;

use crate::contract::ContractResponse;
use crate::msg::MigrateMsg;
use crate::state::accepted_subscriptions;
use crate::state::asset_exchange_storage;
use crate::state::config;
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
        commitment_denom: old_state.commitment_denom,
        investment_denom: old_state.investment_denom,
        capital_denom: old_state.capital_denom,
        capital_per_share: old_state.capital_per_share,
    };
    let new_pending_subscriptions = old_state.pending_review_subs;
    let new_accepted_subscriptions = old_state.accepted_subs;

    let mut storage = asset_exchange_storage(deps.storage);

    for issuance in msg.asset_exchanges {
        storage.save(issuance.subscription.as_bytes(), &vec![issuance.exchange])?;
    }

    config(deps.storage).save(&new_state)?;
    pending_subscriptions(deps.storage).save(&new_pending_subscriptions)?;
    accepted_subscriptions(deps.storage).save(&new_accepted_subscriptions)?;

    Ok(Response::default())
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
struct StateV1_0_1 {
    pub recovery_admin: Addr,
    pub gp: Addr,
    pub acceptable_accreditations: HashSet<String>,
    pub other_required_tags: HashSet<String>,
    pub commitment_denom: String,
    pub investment_denom: String,
    pub capital_denom: String,
    pub capital_per_share: u64,
    pub pending_review_subs: HashSet<Addr>,
    pub accepted_subs: HashSet<Addr>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::contract::tests::default_deps;
    use crate::msg::{AssetExchange, IssueAssetExchange};
    use crate::state::tests::asset_exchange_storage_read;
    use crate::state::{accepted_subscriptions_read, pending_subscriptions_read};
    use cosmwasm_std::testing::mock_env;
    use cosmwasm_std::Addr;
    use cosmwasm_storage::{singleton, singleton_read};

    #[test]
    fn migration() {
        let mut deps = default_deps(None);
        singleton(&mut deps.storage, CONFIG_KEY)
            .save(&StateV1_0_1 {
                recovery_admin: Addr::unchecked("marketpalace"),
                gp: Addr::unchecked("gp"),
                acceptable_accreditations: HashSet::new(),
                other_required_tags: HashSet::new(),
                commitment_denom: String::from("commitment_coin"),
                investment_denom: String::from("investment_coin"),
                capital_denom: String::from("stable_coin"),
                capital_per_share: 100,
                pending_review_subs: vec![Addr::unchecked("sub_2")].into_iter().collect(),
                accepted_subs: vec![Addr::unchecked("sub_1")].into_iter().collect(),
            })
            .unwrap();

        migrate(
            deps.as_mut(),
            mock_env(),
            MigrateMsg {
                subscription_code_id: 1,
                asset_exchanges: vec![IssueAssetExchange {
                    subscription: Addr::unchecked("sub_1"),
                    exchange: AssetExchange {
                        investment: Some(1_000),
                        commitment_in_shares: Some(-1_000),
                        capital: Some(-1_000),
                        date: None,
                    },
                }],
            },
        )
        .unwrap();

        assert_eq!(
            State {
                subscription_code_id: 1,
                recovery_admin: Addr::unchecked("marketpalace"),
                gp: Addr::unchecked("gp"),
                acceptable_accreditations: HashSet::new(),
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
            asset_exchange_storage_read(&deps.storage)
                .load(Addr::unchecked("sub_1").as_bytes())
                .unwrap()
                .len()
        );
    }
}

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

use cosmwasm_std::Addr;

use crate::state::State;

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct InstantiateMsg {
    pub subscription_code_id: u64,
    pub recovery_admin: Addr,
    pub required_attestations: Vec<HashSet<String>>,
    pub capital_denom: String,
    pub capital_per_share: u64,
    pub required_capital_attribute: Option<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub struct MigrateMsg {
    pub subscription_code_id: u64,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum HandleMsg {
    Recover {
        gp: Addr,
    },
    UpdateRequiredAttestations {
        required_attestations: Vec<HashSet<String>>,
    },
    UpdateCapitalDenomination {
        capital_denomination: String,
    },
    MigrateSubscriptions {
        subscriptions: HashSet<Addr>,
    },
    ProposeSubscription {
        initial_commitment: Option<u64>,
    },
    CloseSubscriptions {
        subscriptions: HashSet<Addr>,
    },
    IssueAssetExchanges {
        asset_exchanges: Vec<IssueAssetExchange>,
    },
    CancelAssetExchanges {
        cancellations: Vec<IssueAssetExchange>,
    },
    CompleteAssetExchange {
        exchanges: Vec<AssetExchange>,
        to: Option<Addr>,
        memo: Option<String>,
    },
    UpdateEligibleSubscriptions {
        subscriptions: Vec<Addr>,
    },
    AcceptSubscriptions {
        subscriptions: Vec<AcceptSubscription>,
    },
    IssueWithdrawal {
        to: Addr,
        amount: u64,
        memo: Option<String>,
    },
}

#[derive(Deserialize, Serialize, Clone, Debug, PartialEq, Eq, JsonSchema)]
pub struct AcceptSubscription {
    pub subscription: Addr,
    pub commitment_in_capital: u64,
}

#[derive(Deserialize, Serialize, Clone, Debug, PartialEq, Eq, JsonSchema)]
pub struct IssueAssetExchange {
    pub subscription: Addr,
    pub exchanges: Vec<AssetExchange>,
}

#[derive(Deserialize, Serialize, Clone, Debug, PartialEq, Eq, JsonSchema)]
pub struct AssetExchange {
    #[serde(rename = "inv")]
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(default)]
    pub investment: Option<i64>,
    #[serde(rename = "com")]
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(default)]
    pub commitment_in_shares: Option<i64>,
    #[serde(rename = "cap")]
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(default)]
    pub capital: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(default)]
    pub date: Option<ExchangeDate>,
}

#[derive(Deserialize, Serialize, Clone, Debug, PartialEq, Eq, JsonSchema)]
pub enum ExchangeDate {
    #[serde(rename = "due")]
    Due(u64),
    #[serde(rename = "avl")]
    Available(u64),
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum QueryMsg {
    GetState {},
    GetAllAssetExchanges {},
    GetAssetExchangesForSubscription { subscription: Addr },
}

#[derive(Deserialize, Serialize)]
pub struct RaiseState {
    pub general: State,
    pub pending_subscriptions: HashSet<Addr>,
    pub eligible_subscriptions: HashSet<Addr>,
    pub accepted_subscriptions: HashSet<Addr>,
}

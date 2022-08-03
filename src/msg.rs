use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

use cosmwasm_std::Addr;

use crate::state::{AssetExchange, State};

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct InstantiateMsg {
    pub subscription_code_id: u64,
    pub recovery_admin: Addr,
    pub acceptable_accreditations: HashSet<String>,
    pub other_required_tags: HashSet<String>,
    pub capital_denom: String,
    pub capital_per_share: u64,
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
    ProposeSubscription {},
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
        exchange: AssetExchange,
        to: Option<Addr>,
        memo: Option<String>,
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
    pub commitment: u64,
}

#[derive(Deserialize, Serialize, Clone, Debug, PartialEq, Eq, JsonSchema)]
pub struct IssueAssetExchange {
    pub subscription: Addr,
    pub investment: Option<i64>,
    pub commitment: Option<i64>,
    pub capital: Option<i64>,
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
}

#[derive(Deserialize, Serialize)]
pub struct RaiseState {
    pub general: State,
    pub pending_subscriptions: HashSet<Addr>,
    pub accepted_subscriptions: HashSet<Addr>,
}

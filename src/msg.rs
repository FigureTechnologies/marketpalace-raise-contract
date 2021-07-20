use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

use cosmwasm_std::Addr;

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct InstantiateMsg {
    pub capital_call_code_id: u64,
    pub admin: Addr,
    pub qualified_tags: Vec<String>,
    pub asset_denom: String,
    pub capital_denom: String,
    pub target: u64,
    pub min_commitment: u64,
    pub max_commitment: u64,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum HandleMsg {
    ProposeSubscription {
        subscription: Addr,
    },
    AcceptSubscriptions {
        subscriptions: HashMap<Addr, u64>,
    },
    IssueCalls {
        calls: HashSet<Addr>,
    },
    CloseCalls {
        calls: Vec<Addr>,
    },
    IssueRedemptions {
        redemptions: HashSet<Redemption>,
    },
    IssueDistributions {
        distributions: HashMap<Addr, u64>,
    },
    RedeemCapital {
        to: Addr,
        amount: u64,
        memo: Option<String>,
    },
}

#[derive(Deserialize, Serialize, Clone, Debug, Hash, Eq, JsonSchema)]
pub struct Redemption {
    pub subscription: Addr,
    pub asset: u64,
    pub capital: u64,
}

impl PartialEq for Redemption {
    fn eq(&self, other: &Self) -> bool {
        self.subscription == other.subscription
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum QueryMsg {
    GetStatus {},
    GetSubs {},
    GetCalls {},
}

#[derive(Deserialize, Serialize)]
pub struct Subs {
    pub pending_review: HashSet<Addr>,
    pub accepted: HashSet<Addr>,
}

#[derive(Deserialize, Serialize)]
pub struct Calls {
    pub issued: HashSet<Addr>,
    pub closed: HashSet<Addr>,
}

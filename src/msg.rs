use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::hash::Hash;

use cosmwasm_std::Addr;

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct InstantiateMsg {
    pub subscription_code_id: u64,
    pub admin: Addr,
    pub acceptable_accreditations: HashSet<String>,
    pub other_required_tags: HashSet<String>,
    pub capital_denom: String,
    pub target: u64,
    pub min_commitment: Option<u64>,
    pub max_commitment: Option<u64>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum HandleMsg {
    Recover {
        gp: Addr,
    },
    ProposeSubscription {
        min_commitment: u64,
        max_commitment: u64,
        min_days_of_notice: Option<u16>,
    },
    AcceptSubscriptions {
        subscriptions: HashSet<AcceptSubscription>,
    },
    IssueCapitalCalls {
        calls: HashSet<Call>,
    },
    CloseCapitalCalls {
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

#[derive(Deserialize, Serialize, Clone, Debug, Eq, JsonSchema)]
pub struct AcceptSubscription {
    pub subscription: Addr,
    pub commitment: u64,
}

impl PartialEq for AcceptSubscription {
    fn eq(&self, other: &Self) -> bool {
        self.subscription == other.subscription
    }
}

impl Hash for AcceptSubscription {
    fn hash<H>(&self, state: &mut H)
    where
        H: std::hash::Hasher,
    {
        self.subscription.hash(state);
    }
}

#[derive(Deserialize, Serialize, Clone, Debug, Eq, JsonSchema)]
pub struct Call {
    pub subscription: Addr,
    pub amount: u64,
    pub days_of_notice: Option<u16>,
}

impl PartialEq for Call {
    fn eq(&self, other: &Self) -> bool {
        self.subscription == other.subscription
    }
}

impl Hash for Call {
    fn hash<H>(&self, state: &mut H)
    where
        H: std::hash::Hasher,
    {
        self.subscription.hash(state);
    }
}

#[derive(Deserialize, Serialize, Clone, Debug, Eq, JsonSchema)]
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

impl Hash for Redemption {
    fn hash<H>(&self, state: &mut H)
    where
        H: std::hash::Hasher,
    {
        self.subscription.hash(state)
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum QueryMsg {
    GetStatus {},
    GetTerms {},
    GetSubs {},
    GetCalls {},
}

#[derive(Deserialize, Serialize)]
pub struct Terms {
    pub acceptable_accreditations: HashSet<String>,
    pub other_required_tags: HashSet<String>,
    pub asset_denom: String,
    pub capital_denom: String,
    pub target: u64,
    pub min_commitment: Option<u64>,
    pub max_commitment: Option<u64>,
}

#[derive(Deserialize, Serialize)]
pub struct Subs {
    pub pending_review: HashSet<Addr>,
    pub accepted: HashSet<Addr>,
}

#[derive(Deserialize, Serialize)]
pub struct Calls {
    pub issued: HashSet<Call>,
}

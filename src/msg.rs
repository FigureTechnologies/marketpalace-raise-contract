use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::hash::Hash;

use cosmwasm_std::Addr;

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
    ProposeSubscription {
        min_commitment: u64,
        max_commitment: u64,
    },
    CloseSubscriptions {
        subscriptions: HashSet<Addr>,
    },
    CloseRemainingCommitment {},
    AcceptSubscriptions {
        subscriptions: HashSet<AcceptSubscription>,
    },
    UpdateCommitments {
        commitment_updates: HashSet<CommitmentUpdate>,
    },
    AcceptCommitmentUpdate {},
    IssueCapitalCalls {
        calls: HashSet<CapitalCall>,
    },
    CancelCapitalCalls {
        subscriptions: HashSet<Addr>,
    },
    ClaimInvestment {},
    IssueRedemptions {
        redemptions: HashSet<Redemption>,
    },
    CancelRedemptions {
        subscriptions: HashSet<Addr>,
    },
    ClaimRedemption {
        to: Addr,
        memo: Option<String>,
    },
    IssueDistributions {
        distributions: HashSet<Distribution>,
    },
    CancelDistributions {
        subscriptions: HashSet<Addr>,
    },
    ClaimDistribution {
        to: Addr,
        memo: Option<String>,
    },
    IssueWithdrawal {
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
pub struct CommitmentUpdate {
    pub subscription: Addr,
    pub change_by_amount: i64,
}

impl PartialEq for CommitmentUpdate {
    fn eq(&self, other: &Self) -> bool {
        self.subscription == other.subscription
    }
}

impl Hash for CommitmentUpdate {
    fn hash<H>(&self, state: &mut H)
    where
        H: std::hash::Hasher,
    {
        self.subscription.hash(state);
    }
}

#[derive(Deserialize, Serialize, Clone, Eq, Debug, JsonSchema)]
pub struct CapitalCall {
    pub subscription: Addr,
    pub amount: u64,
    pub due_epoch_seconds: Option<u64>,
}

impl PartialEq for CapitalCall {
    fn eq(&self, other: &Self) -> bool {
        self.subscription == other.subscription
    }
}

impl Hash for CapitalCall {
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
    pub available_epoch_seconds: Option<u64>,
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

#[derive(Deserialize, Serialize, Clone, Debug, Eq, JsonSchema)]
pub struct Distribution {
    pub subscription: Addr,
    pub amount: u64,
    pub available_epoch_seconds: Option<u64>,
}

impl PartialEq for Distribution {
    fn eq(&self, other: &Self) -> bool {
        self.subscription == other.subscription
    }
}

impl Hash for Distribution {
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
    GetTerms {},
    GetSubs {},
}

#[derive(Deserialize, Serialize)]
pub struct Terms {
    pub acceptable_accreditations: HashSet<String>,
    pub other_required_tags: HashSet<String>,
    pub commitment_denom: String,
    pub investment_denom: String,
    pub capital_denom: String,
    pub capital_per_share: u64,
}

#[derive(Deserialize, Serialize)]
pub struct Subs {
    pub pending_review: HashSet<Addr>,
    pub accepted: HashSet<Addr>,
}

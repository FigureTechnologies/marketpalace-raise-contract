use serde::{Deserialize, Serialize};

use cosmwasm_std::Addr;

#[derive(Deserialize, Serialize)]
pub struct CallTerms {
    pub subscription: Addr,
    pub raise: Addr,
}

#[derive(Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CallQueryMsg {
    GetTerms {},
}
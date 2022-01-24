use crate::error::contract_error;
use crate::error::ContractError;
use crate::msg::InstantiateMsg;
use crate::state::config;
use crate::state::{State, Status};
use cosmwasm_std::{entry_point, CosmosMsg, DepsMut, Env, MessageInfo, Response, StdResult};
use provwasm_std::{
    activate_marker, create_marker, finalize_marker, grant_marker_access, MarkerAccess, MarkerType,
    ProvenanceMsg,
};
use std::collections::HashSet;

// Note, you can use StdResult in some functions where you do not
// make use of the custom errors
#[entry_point]
pub fn instantiate(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    msg: InstantiateMsg,
) -> Result<Response<ProvenanceMsg>, ContractError> {
    let state = State {
        subscription_code_id: msg.subscription_code_id,
        status: Status::Active,
        recovery_admin: msg.recovery_admin,
        gp: info.sender,
        acceptable_accreditations: msg.acceptable_accreditations,
        other_required_tags: msg.other_required_tags,
        commitment_denom: format!("{}.commitment", env.contract.address),
        investment_denom: format!("{}.investment", env.contract.address),
        capital_denom: msg.capital_denom,
        target: msg.target,
        capital_per_share: msg.capital_per_share,
        min_commitment: msg.min_commitment,
        max_commitment: msg.max_commitment,
        sequence: 0,
        pending_review_subs: HashSet::new(),
        accepted_subs: HashSet::new(),
        issued_withdrawals: HashSet::new(),
    };

    if state.not_evenly_divisble(msg.target) {
        return contract_error("target must be evenly divisible by capital per share");
    }

    if let Some(min_commitment) = msg.min_commitment {
        if state.not_evenly_divisble(min_commitment) {
            return contract_error("min commitment must be evenly divisible by capital per share");
        }
    }

    if let Some(max_commitment) = msg.max_commitment {
        if state.not_evenly_divisble(max_commitment) {
            return contract_error("max commitment must be evenly divisible by capital per share");
        }
    }

    config(deps.storage).save(&state)?;

    let create_and_activate_marker = |denom: String| -> StdResult<Vec<CosmosMsg<ProvenanceMsg>>> {
        Ok(vec![
            create_marker(
                state.capital_to_shares(state.target) as u128,
                denom.clone(),
                MarkerType::Coin,
            )?,
            grant_marker_access(
                denom.clone(),
                env.contract.address.clone(),
                vec![
                    MarkerAccess::Admin,
                    MarkerAccess::Mint,
                    MarkerAccess::Burn,
                    MarkerAccess::Withdraw,
                ],
            )?,
            finalize_marker(denom.clone())?,
            activate_marker(denom)?,
        ])
    };

    Ok(Response::default()
        .add_messages(create_and_activate_marker(state.commitment_denom.clone())?)
        .add_messages(create_and_activate_marker(state.investment_denom.clone())?))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::msg::QueryMsg;
    use crate::msg::Terms;
    use crate::query::query;
    use cosmwasm_std::testing::{mock_env, mock_info};
    use cosmwasm_std::{from_binary, Addr};
    use provwasm_mocks::mock_dependencies;

    #[test]
    fn initialization() {
        let mut deps = mock_dependencies(&[]);
        let info = mock_info("gp", &[]);

        // instantiate and verify we have 3 messages (create, grant, & activate)
        let res = instantiate(
            deps.as_mut(),
            mock_env(),
            info,
            InstantiateMsg {
                subscription_code_id: 0,
                recovery_admin: Addr::unchecked("marketpalace"),
                acceptable_accreditations: HashSet::new(),
                other_required_tags: HashSet::new(),
                capital_denom: String::from("stable_coin"),
                target: 5_000_000,
                capital_per_share: 100,
                min_commitment: Some(10_000),
                max_commitment: Some(100_000),
            },
        )
        .unwrap();
        assert_eq!(8, res.messages.len());

        // verify raise is in active status
        let res = query(deps.as_ref(), mock_env(), QueryMsg::GetStatus {}).unwrap();
        let status: Status = from_binary(&res).unwrap();
        assert_eq!(Status::Active, status);

        // verify that terms of raise are correct
        let res = query(deps.as_ref(), mock_env(), QueryMsg::GetTerms {}).unwrap();
        let terms: Terms = from_binary(&res).unwrap();
        assert_eq!(0, terms.acceptable_accreditations.len());
        assert_eq!(0, terms.other_required_tags.len());
        assert_eq!("cosmos2contract.commitment", terms.commitment_denom);
        assert_eq!("cosmos2contract.investment", terms.investment_denom);
        assert_eq!("stable_coin", terms.capital_denom);
        assert_eq!(5_000_000, terms.target);
        assert_eq!(10_000, terms.min_commitment.unwrap());
        assert_eq!(100_000, terms.max_commitment.unwrap());
    }

    #[test]
    fn init_with_bad_target() {
        let mut deps = mock_dependencies(&[]);
        let info = mock_info("gp", &[]);

        // instantiate and verify we have 3 messages (create, grant, & activate)
        let res = instantiate(
            deps.as_mut(),
            mock_env(),
            info,
            InstantiateMsg {
                subscription_code_id: 0,
                recovery_admin: Addr::unchecked("marketpalace"),
                acceptable_accreditations: HashSet::new(),
                other_required_tags: HashSet::new(),
                capital_denom: String::from("stable_coin"),
                target: 5_000_001,
                capital_per_share: 100,
                min_commitment: Some(10_000),
                max_commitment: Some(100_000),
            },
        );
        assert_eq!(true, res.is_err());
    }

    #[test]
    fn init_with_bad_min() {
        let mut deps = mock_dependencies(&[]);
        let info = mock_info("gp", &[]);

        // instantiate and verify we have 3 messages (create, grant, & activate)
        let res = instantiate(
            deps.as_mut(),
            mock_env(),
            info,
            InstantiateMsg {
                subscription_code_id: 0,
                recovery_admin: Addr::unchecked("marketpalace"),
                acceptable_accreditations: HashSet::new(),
                other_required_tags: HashSet::new(),
                capital_denom: String::from("stable_coin"),
                target: 5_000_000,
                capital_per_share: 100,
                min_commitment: Some(10_001),
                max_commitment: Some(100_000),
            },
        );
        assert_eq!(true, res.is_err());
    }

    #[test]
    fn init_with_bad_max() {
        let mut deps = mock_dependencies(&[]);
        let info = mock_info("gp", &[]);

        // instantiate and verify we have 3 messages (create, grant, & activate)
        let res = instantiate(
            deps.as_mut(),
            mock_env(),
            info,
            InstantiateMsg {
                subscription_code_id: 0,
                recovery_admin: Addr::unchecked("marketpalace"),
                acceptable_accreditations: HashSet::new(),
                other_required_tags: HashSet::new(),
                capital_denom: String::from("stable_coin"),
                target: 5_000_000,
                capital_per_share: 100,
                min_commitment: Some(10_000),
                max_commitment: Some(100_001),
            },
        );
        assert_eq!(true, res.is_err());
    }
}

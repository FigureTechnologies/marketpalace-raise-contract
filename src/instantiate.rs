use crate::contract::ContractResponse;
use crate::msg::InstantiateMsg;
use crate::state::config;
use crate::state::State;
use crate::version::CONTRACT_NAME;
use crate::version::CONTRACT_VERSION;
use cosmwasm_std::{entry_point, CosmosMsg, DepsMut, Env, MessageInfo, Response, StdResult};
use cw2::set_contract_version;
use provwasm_std::ProvenanceQuery;
use provwasm_std::{
    activate_marker, create_marker, finalize_marker, grant_marker_access, MarkerAccess, MarkerType,
    ProvenanceMsg,
};
use std::collections::HashSet;

// Note, you can use StdResult in some functions where you do not
// make use of the custom errors
#[entry_point]
pub fn instantiate(
    deps: DepsMut<ProvenanceQuery>,
    env: Env,
    info: MessageInfo,
    msg: InstantiateMsg,
) -> ContractResponse {
    set_contract_version(deps.storage, CONTRACT_NAME, CONTRACT_VERSION)?;

    let state = State {
        subscription_code_id: msg.subscription_code_id,
        recovery_admin: msg.recovery_admin,
        gp: info.sender,
        acceptable_accreditations: msg.acceptable_accreditations,
        other_required_tags: msg.other_required_tags,
        commitment_denom: format!("{}.commitment", env.contract.address),
        investment_denom: format!("{}.investment", env.contract.address),
        capital_denom: msg.capital_denom,
        capital_per_share: msg.capital_per_share,
        pending_review_subs: HashSet::new(),
        accepted_subs: HashSet::new(),
    };

    config(deps.storage).save(&state)?;

    let create_and_activate_marker = |denom: String| -> StdResult<Vec<CosmosMsg<ProvenanceMsg>>> {
        Ok(vec![
            create_marker(0, denom.clone(), MarkerType::Coin)?,
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
        .add_messages(create_and_activate_marker(state.investment_denom)?))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mock::marker_msg;
    use crate::mock::msg_at_index;
    use crate::msg::QueryMsg;
    use crate::msg::Terms;
    use crate::query::query;
    use cosmwasm_std::testing::MOCK_CONTRACT_ADDR;
    use cosmwasm_std::testing::{mock_env, mock_info};
    use cosmwasm_std::{from_binary, Addr};
    use provwasm_mocks::mock_dependencies;
    use provwasm_std::MarkerMsgParams;

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
                capital_per_share: 100,
            },
        )
        .unwrap();

        // verify that 8 messages are sent to configure 2 new markers
        assert_eq!(8, res.messages.len());
        assert!(matches!(
            marker_msg(msg_at_index(&res, 0)),
            MarkerMsgParams::CreateMarker { .. }
        ));
        assert!(matches!(
            marker_msg(msg_at_index(&res, 1)),
            MarkerMsgParams::GrantMarkerAccess { .. }
        ));
        assert!(matches!(
            marker_msg(msg_at_index(&res, 2)),
            MarkerMsgParams::FinalizeMarker { .. }
        ));
        assert!(matches!(
            marker_msg(msg_at_index(&res, 3)),
            MarkerMsgParams::ActivateMarker { .. }
        ));
        assert!(matches!(
            marker_msg(msg_at_index(&res, 4)),
            MarkerMsgParams::CreateMarker { .. }
        ));
        assert!(matches!(
            marker_msg(msg_at_index(&res, 5)),
            MarkerMsgParams::GrantMarkerAccess { .. }
        ));
        assert!(matches!(
            marker_msg(msg_at_index(&res, 6)),
            MarkerMsgParams::FinalizeMarker { .. }
        ));
        assert!(matches!(
            marker_msg(msg_at_index(&res, 7)),
            MarkerMsgParams::ActivateMarker { .. }
        ));

        // verify that terms of raise are correct
        let res = query(deps.as_ref(), mock_env(), QueryMsg::GetTerms {}).unwrap();
        let terms: Terms = from_binary(&res).unwrap();
        assert_eq!(0, terms.acceptable_accreditations.len());
        assert_eq!(0, terms.other_required_tags.len());
        assert_eq!(
            format!("{}.commitment", MOCK_CONTRACT_ADDR),
            terms.commitment_denom
        );
        assert_eq!(
            format!("{}.investment", MOCK_CONTRACT_ADDR),
            terms.investment_denom
        );
        assert_eq!("stable_coin", terms.capital_denom);
    }
}

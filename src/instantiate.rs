use crate::contract::ContractResponse;
use crate::error::contract_error;
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

// Note, you can use StdResult in some functions where you do not
// make use of the custom errors
#[entry_point]
pub fn instantiate(
    deps: DepsMut<ProvenanceQuery>,
    env: Env,
    info: MessageInfo,
    msg: InstantiateMsg,
) -> ContractResponse {
    if msg.like_capital_denoms.is_empty() {
        return contract_error("at least 1 like capital denom required");
    }

    set_contract_version(deps.storage, CONTRACT_NAME, CONTRACT_VERSION)?;

    let state = State {
        subscription_code_id: msg.subscription_code_id,
        recovery_admin: msg.recovery_admin,
        gp: info.sender,
        required_attestations: msg.required_attestations,
        commitment_denom: format!("{}.commitment", env.contract.address),
        investment_denom: format!("{}.investment", env.contract.address),
        like_capital_denoms: msg.like_capital_denoms,
        capital_per_share: msg.capital_per_share,
        required_capital_attribute: msg.required_capital_attribute,
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
    use crate::msg::RaiseState;
    use crate::query::query;
    use cosmwasm_std::coin;
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
                required_attestations: vec![],
                like_capital_denoms: vec![String::from("stable_coin")],
                capital_per_share: 100,
                required_capital_attribute: None,
            },
        )
        .unwrap();

        // verify that 8 messages are sent to configure 2 new markers
        assert_eq!(8, res.messages.len());

        // verify create commitment marker
        let commitment_denom = format!("{}.commitment", MOCK_CONTRACT_ADDR);
        assert_eq!(
            &MarkerMsgParams::CreateMarker {
                coin: coin(0, commitment_denom.clone()),
                marker_type: MarkerType::Coin
            },
            marker_msg(msg_at_index(&res, 0)),
        );

        // verify grant commitment marker permissions
        assert_eq!(
            &MarkerMsgParams::GrantMarkerAccess {
                denom: commitment_denom.clone(),
                address: Addr::unchecked(MOCK_CONTRACT_ADDR),
                permissions: vec![
                    MarkerAccess::Admin,
                    MarkerAccess::Mint,
                    MarkerAccess::Burn,
                    MarkerAccess::Withdraw,
                ],
            },
            marker_msg(msg_at_index(&res, 1)),
        );

        // verify finalize commitment marker
        assert_eq!(
            &MarkerMsgParams::FinalizeMarker {
                denom: commitment_denom.clone()
            },
            marker_msg(msg_at_index(&res, 2)),
        );

        // verify activate commitment marker
        assert_eq!(
            &MarkerMsgParams::ActivateMarker {
                denom: commitment_denom
            },
            marker_msg(msg_at_index(&res, 3)),
        );

        // verify create investment marker
        let investment_denom = format!("{}.investment", MOCK_CONTRACT_ADDR);
        assert_eq!(
            &MarkerMsgParams::CreateMarker {
                coin: coin(0, investment_denom.clone()),
                marker_type: MarkerType::Coin
            },
            marker_msg(msg_at_index(&res, 4)),
        );

        // verify grant investment marker permissions
        assert_eq!(
            &MarkerMsgParams::GrantMarkerAccess {
                denom: investment_denom.clone(),
                address: Addr::unchecked(MOCK_CONTRACT_ADDR),
                permissions: vec![
                    MarkerAccess::Admin,
                    MarkerAccess::Mint,
                    MarkerAccess::Burn,
                    MarkerAccess::Withdraw,
                ],
            },
            marker_msg(msg_at_index(&res, 5)),
        );

        // verify finalize investment marker
        assert_eq!(
            &MarkerMsgParams::FinalizeMarker {
                denom: investment_denom.clone()
            },
            marker_msg(msg_at_index(&res, 6)),
        );

        // verify activate investment marker
        assert_eq!(
            &MarkerMsgParams::ActivateMarker {
                denom: investment_denom
            },
            marker_msg(msg_at_index(&res, 7)),
        );

        // verify that terms of raise are correct
        let res = query(deps.as_ref(), mock_env(), QueryMsg::GetState {}).unwrap();
        let state: RaiseState = from_binary(&res).unwrap();
        assert_eq!(0, state.general.subscription_code_id);
        assert_eq!("marketpalace", state.general.recovery_admin);
        assert_eq!("gp", state.general.gp);
        assert_eq!(0, state.general.required_attestations.len());
        assert_eq!(
            format!("{}.commitment", MOCK_CONTRACT_ADDR),
            state.general.commitment_denom
        );
        assert_eq!(
            format!("{}.investment", MOCK_CONTRACT_ADDR),
            state.general.investment_denom
        );
        assert_eq!(
            "stable_coin",
            state.general.like_capital_denoms.first().unwrap()
        );
        assert_eq!(100, state.general.capital_per_share);
    }

    #[test]
    fn initialization_without_cap_denoms() {
        let mut deps = mock_dependencies(&[]);
        let info = mock_info("gp", &[]);

        let res = instantiate(
            deps.as_mut(),
            mock_env(),
            info,
            InstantiateMsg {
                subscription_code_id: 0,
                recovery_admin: Addr::unchecked("marketpalace"),
                required_attestations: vec![],
                like_capital_denoms: vec![],
                capital_per_share: 100,
                required_capital_attribute: None,
            },
        );
        assert!(res.is_err());
    }
}

use crate::contract::ContractResponse;
use crate::error::contract_error;
use crate::error::ContractError;
use crate::state::config;
use crate::state::config_read;
use cosmwasm_std::Addr;
use cosmwasm_std::DepsMut;
use cosmwasm_std::MessageInfo;
use cosmwasm_std::Response;

pub fn try_recover(deps: DepsMut, info: MessageInfo, gp: Addr) -> ContractResponse {
    let state = config_read(deps.storage).load()?;

    if info.sender != state.recovery_admin {
        return contract_error("only admin can recover raise");
    }

    config(deps.storage).update(|mut state| -> Result<_, ContractError> {
        state.gp = gp;
        Ok(state)
    })?;

    Ok(Response::default())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::contract::execute;
    use crate::contract::tests::default_deps;
    use crate::msg::HandleMsg;
    use cosmwasm_std::testing::mock_env;
    use cosmwasm_std::testing::mock_info;

    #[test]
    fn recover() {
        let mut deps = default_deps(None);

        execute(
            deps.as_mut(),
            mock_env(),
            mock_info("marketpalace", &vec![]),
            HandleMsg::Recover {
                gp: Addr::unchecked("gp_2"),
            },
        )
        .unwrap();

        // verify that gp has been updated
        let state = config_read(&deps.storage).load().unwrap();
        assert_eq!("gp_2", state.gp);
    }

    #[test]
    fn fail_bad_actor_recover() {
        let mut deps = default_deps(None);

        let res = execute(
            deps.as_mut(),
            mock_env(),
            mock_info("bad_actor", &vec![]),
            HandleMsg::Recover {
                gp: Addr::unchecked("bad_actor"),
            },
        );
        assert_eq!(true, res.is_err());

        // verify that gp has NOT been updated
        let state = config_read(&deps.storage).load().unwrap();
        assert_eq!("gp", state.gp);
    }
}

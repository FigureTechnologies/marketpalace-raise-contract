use crate::error::contract_error;
use crate::exchange_asset::try_cancel_asset_exchanges;
use crate::exchange_asset::try_complete_asset_exchange;
use crate::exchange_asset::try_issue_asset_exchanges;
use crate::recover::try_recover;
use crate::state::pending_subscriptions;
use crate::state::pending_subscriptions_read;
use crate::subscribe::try_accept_subscriptions;
use crate::subscribe::try_close_subscriptions;
use crate::subscribe::try_propose_subscription;
use cosmwasm_std::{
    coins, entry_point, Addr, Attribute, BankMsg, DepsMut, Env, Event, MessageInfo, Reply,
    Response, SubMsgResult,
};
use provwasm_std::ProvenanceMsg;
use provwasm_std::ProvenanceQuery;

use crate::error::ContractError;
use crate::msg::HandleMsg;
use crate::state::config;

pub type ContractResponse = Result<Response<ProvenanceMsg>, ContractError>;

#[entry_point]
pub fn reply(deps: DepsMut, _env: Env, msg: Reply) -> Result<Response, ContractError> {
    // look for a contract address from instantiating subscription contract
    if let SubMsgResult::Ok(response) = msg.result {
        if let Some(contract_address) = contract_address(&response.events) {
            let mut pending = pending_subscriptions_read(deps.storage)
                .may_load()?
                .unwrap_or_default();
            pending.insert(contract_address);
            pending_subscriptions(deps.storage).save(&pending)?;
        } else {
            return contract_error("no contract address found");
        }
    } else {
        return contract_error("subscription contract instantiation failed");
    }

    Ok(Response::default())
}

fn contract_address(events: &[Event]) -> Option<Addr> {
    events.first().and_then(|event| {
        event
            .attributes
            .iter()
            .find(|attr| attr.key == "_contract_address")
            .map(|attr| Addr::unchecked(attr.value.clone()))
    })
}

#[entry_point]
pub fn execute(
    deps: DepsMut<ProvenanceQuery>,
    env: Env,
    info: MessageInfo,
    msg: HandleMsg,
) -> ContractResponse {
    match msg {
        HandleMsg::Recover { gp } => try_recover(deps, info, gp),
        HandleMsg::ProposeSubscription {} => try_propose_subscription(deps, env, info),
        HandleMsg::CloseSubscriptions { subscriptions } => {
            try_close_subscriptions(deps, info, subscriptions)
        }
        HandleMsg::AcceptSubscriptions { subscriptions } => {
            try_accept_subscriptions(deps, info, subscriptions)
        }
        HandleMsg::IssueAssetExchanges { asset_exchanges } => {
            try_issue_asset_exchanges(deps, info, asset_exchanges)
        }
        HandleMsg::CancelAssetExchanges { cancellations } => {
            try_cancel_asset_exchanges(deps, info, cancellations)
        }
        HandleMsg::CompleteAssetExchange { exchange, to, memo } => {
            try_complete_asset_exchange(deps, env, info, exchange, to, memo)
        }
        HandleMsg::IssueWithdrawal { to, amount, memo } => {
            try_issue_withdrawal(deps, info, env, to, amount, memo)
        }
    }
}

pub fn try_issue_withdrawal(
    deps: DepsMut<ProvenanceQuery>,
    info: MessageInfo,
    _env: Env,
    to: Addr,
    amount: u64,
    memo: Option<String>,
) -> ContractResponse {
    let state = config(deps.storage).load()?;

    if info.sender != state.gp {
        return contract_error("only gp can redeem capital");
    }

    let send = BankMsg::Send {
        to_address: to.to_string(),
        amount: coins(amount as u128, state.capital_denom),
    };

    let attributes = match memo {
        Some(memo) => {
            vec![Attribute {
                key: String::from("memo"),
                value: memo,
            }]
        }
        None => vec![],
    };

    Ok(Response::new().add_message(send).add_attributes(attributes))
}

#[cfg(test)]
pub mod tests {
    use super::*;
    use crate::mock::msg_at_index;
    use crate::mock::send_args;
    use crate::state::State;
    use cosmwasm_std::testing::{mock_env, mock_info, MockApi, MockStorage};
    use cosmwasm_std::{Addr, OwnedDeps};
    use provwasm_mocks::{mock_dependencies, ProvenanceMockQuerier};

    pub fn default_deps(
        update_state: Option<fn(&mut State)>,
    ) -> OwnedDeps<MockStorage, MockApi, ProvenanceMockQuerier, ProvenanceQuery> {
        let mut deps = mock_dependencies(&[]);

        let mut state = State::test_default();
        if let Some(update) = update_state {
            update(&mut state);
        }
        config(&mut deps.storage).save(&state).unwrap();

        deps
    }

    #[test]
    fn issue_withdrawal() {
        let mut deps = default_deps(None);

        let res = execute(
            deps.as_mut(),
            mock_env(),
            mock_info("gp", &[]),
            HandleMsg::IssueWithdrawal {
                to: Addr::unchecked("omni"),
                amount: 10_000,
                memo: None,
            },
        )
        .unwrap();

        // verify that send message is sent
        assert_eq!(1, res.messages.len());
        let (to_address, coins) = send_args(msg_at_index(&res, 0));
        assert_eq!("omni", to_address);
        assert_eq!(10_000, coins.first().unwrap().amount.u128());
    }

    #[test]
    fn issue_withdrawal_bad_actor() {
        let mut deps = default_deps(None);

        let res = execute(
            deps.as_mut(),
            mock_env(),
            mock_info("bad_actor", &[]),
            HandleMsg::IssueWithdrawal {
                to: Addr::unchecked("omni"),
                amount: 10_000,
                memo: None,
            },
        );
        assert!(res.is_err());
    }
}

use std::marker::PhantomData;

use cosmwasm_std::testing::{MockApi, MockQuerier, MockStorage, MOCK_CONTRACT_ADDR};
use cosmwasm_std::BankMsg;
use cosmwasm_std::CosmosMsg;
use cosmwasm_std::Response;
use cosmwasm_std::WasmMsg;
use cosmwasm_std::{from_binary, Addr};
use cosmwasm_std::{
    from_slice, Binary, Coin, ContractResult, OwnedDeps, Querier, QueryRequest, SystemError,
    SystemResult, WasmQuery,
};
use provwasm_mocks::{must_read_binary_file, ProvenanceMockQuerier};
use provwasm_std::ProvenanceMsg;
use provwasm_std::ProvenanceMsgParams;
use provwasm_std::ProvenanceQuery;
use provwasm_std::{Marker, MarkerMsgParams};
use serde::de::DeserializeOwned;

pub type MockWasmSmartHandler = fn(String, Binary) -> SystemResult<ContractResult<Binary>>;
pub type MockBankBalanceHandler = fn(String, String) -> SystemResult<ContractResult<Binary>>;

pub struct MockContractQuerier {
    pub base: ProvenanceMockQuerier,
    pub wasm_smart_handler: MockWasmSmartHandler,
}

impl Querier for MockContractQuerier {
    fn raw_query(&self, bin_request: &[u8]) -> SystemResult<ContractResult<Binary>> {
        return match from_slice::<QueryRequest<ProvenanceQuery>>(bin_request) {
            Ok(value) => match value.clone() {
                QueryRequest::Wasm(msg) => match msg {
                    WasmQuery::Smart { contract_addr, msg } => {
                        (self.wasm_smart_handler)(contract_addr, msg)
                    }
                    _ => self.base.handle_query(&value),
                },
                _ => self.base.handle_query(&value),
            },
            Err(e) => SystemResult::Err(SystemError::InvalidRequest {
                error: format!("Parsing query request: {}", e),
                request: bin_request.into(),
            }),
        };
    }
}

pub fn wasm_smart_mock_dependencies(
    contract_balance: &[Coin],
    wasm_smart_handler: MockWasmSmartHandler,
) -> OwnedDeps<MockStorage, MockApi, MockContractQuerier, ProvenanceQuery> {
    let base =
        ProvenanceMockQuerier::new(MockQuerier::new(&[(MOCK_CONTRACT_ADDR, contract_balance)]));

    OwnedDeps {
        storage: MockStorage::default(),
        api: MockApi::default(),
        querier: MockContractQuerier {
            base,
            wasm_smart_handler,
        },
        custom_query_type: PhantomData,
    }
}

pub fn msg_at_index(res: &Response<ProvenanceMsg>, i: usize) -> &CosmosMsg<ProvenanceMsg> {
    &res.messages.get(i).unwrap().msg
}

pub fn bank_msg(msg: &CosmosMsg<ProvenanceMsg>) -> &BankMsg {
    if let CosmosMsg::Bank(msg) = msg {
        msg
    } else {
        panic!("not a cosmos bank message!")
    }
}

pub fn send_args(msg: &CosmosMsg<ProvenanceMsg>) -> (&String, &Vec<Coin>) {
    if let BankMsg::Send { to_address, amount } = bank_msg(msg) {
        (to_address, amount)
    } else {
        panic!("not a send bank message!")
    }
}

pub fn marker_msg(msg: &CosmosMsg<ProvenanceMsg>) -> &MarkerMsgParams {
    if let CosmosMsg::Custom(msg) = msg {
        if let ProvenanceMsgParams::Marker(params) = &msg.params {
            params
        } else {
            panic!("not a marker message!")
        }
    } else {
        panic!("not a cosmos custom message!")
    }
}

pub fn marker_transfer_msg(msg: &CosmosMsg<ProvenanceMsg>) -> &MarkerMsgParams {
    if let CosmosMsg::Custom(msg) = msg {
        if let ProvenanceMsgParams::Marker(params) = &msg.params {
            if let MarkerMsgParams::TransferMarkerCoins { coin, to, from } = params {
                params
            } else {
                panic!("not a marker transfer message!")
            }
        } else {
            panic!("not a marker message!")
        }
    } else {
        panic!("not a cosmos custom message!")
    }
}

pub fn mint_args(msg: &CosmosMsg<ProvenanceMsg>) -> &Coin {
    if let MarkerMsgParams::MintMarkerSupply { coin } = marker_msg(msg) {
        coin
    } else {
        panic!("not a mint marker message!")
    }
}

pub fn withdraw_args(msg: &CosmosMsg<ProvenanceMsg>) -> (&String, &Coin, &Addr) {
    if let MarkerMsgParams::WithdrawCoins {
        marker_denom,
        coin,
        recipient,
    } = marker_msg(msg)
    {
        (marker_denom, coin, recipient)
    } else {
        panic!("not a withdraw marker message!")
    }
}

pub fn burn_args(msg: &CosmosMsg<ProvenanceMsg>) -> &Coin {
    if let MarkerMsgParams::BurnMarkerSupply { coin } = marker_msg(msg) {
        coin
    } else {
        panic!("not a mint burn message!")
    }
}

pub fn wasm_msg(msg: &CosmosMsg<ProvenanceMsg>) -> &WasmMsg {
    if let CosmosMsg::Wasm(msg) = msg {
        msg
    } else {
        panic!("not a cosmos wasm message")
    }
}

pub fn instantiate_args<T: DeserializeOwned>(
    msg: &CosmosMsg<ProvenanceMsg>,
) -> (&Option<String>, &u64, T, &Vec<Coin>, &String) {
    if let WasmMsg::Instantiate {
        admin,
        code_id,
        msg,
        funds,
        label,
    } = wasm_msg(msg)
    {
        (admin, code_id, from_binary::<T>(msg).unwrap(), funds, label)
    } else {
        panic!("not a wasm execute message")
    }
}

pub fn execute_args<T: DeserializeOwned>(
    msg: &CosmosMsg<ProvenanceMsg>,
) -> (&String, T, &Vec<Coin>) {
    if let WasmMsg::Execute {
        contract_addr,
        msg,
        funds,
    } = wasm_msg(msg)
    {
        (contract_addr, from_binary::<T>(msg).unwrap(), funds)
    } else {
        panic!("not a wasm execute message")
    }
}

pub fn load_markers(querier: &mut ProvenanceMockQuerier) {
    let get_marker = |name: &str| -> Marker {
        let bin = must_read_binary_file(&format!("testdata/{}_marker.json", name));
        from_binary(&bin).unwrap()
    };

    querier.with_markers(vec![
        get_marker("commitment"),
        get_marker("investment"),
        get_marker("capital_coin"),
        get_marker("restricted_capital_coin"),
    ]);
}

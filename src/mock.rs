use cosmwasm_std::testing::{MockApi, MockQuerier, MockStorage, MOCK_CONTRACT_ADDR};
use cosmwasm_std::{
    from_slice, Binary, Coin, ContractResult, OwnedDeps, Querier, QueryRequest, SystemError,
    SystemResult, WasmQuery,
};

use provwasm_mocks::ProvenanceMockQuerier;
use provwasm_std::ProvenanceQuery;

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
) -> OwnedDeps<MockStorage, MockApi, MockContractQuerier> {
    let base =
        ProvenanceMockQuerier::new(MockQuerier::new(&[(MOCK_CONTRACT_ADDR, contract_balance)]));

    OwnedDeps {
        storage: MockStorage::default(),
        api: MockApi::default(),
        querier: MockContractQuerier {
            base,
            wasm_smart_handler,
        },
    }
}

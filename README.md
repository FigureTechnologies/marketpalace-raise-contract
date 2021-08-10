# Marketpalace Raise Contract

### local testing
1. `make optimize`
2. `
provenanced -t tx wasm store ./artifacts/marketpalace_raise_contract.wasm \
--source "https://github.com/FigureTechnologies/marketpalace-raise-contract" \
--builder "cosmwasm/rust-optimizer:0.11.3" \
--from $(faucet) \
--home $N0 \
--chain-id chain-local \
--gas auto --gas-prices 1905nhash --gas-adjustment 2 \
--yes
`
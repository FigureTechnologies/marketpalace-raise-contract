# Marketpalace Raise Contract

### build
1. make
2. make optimize

### store contract on chain
    provenanced -t tx wasm store ./artifacts/marketpalace_raise_contract.wasm \
      --source "https://github.com/FigureTechnologies/marketpalace-raise-contract" \
      --builder "cosmwasm/rust-optimizer:0.11.3" \
      --from $(faucet) \
      --home $N0 \
      --chain-id chain-local \
      --gas auto --gas-prices 1905nhash --gas-adjustment 2 \
      --yes

### instantiate contract
    provenanced -t tx wasm instantiate 1 '{}' \
      --label test \
      --from $(faucet) \
      --home $N0 \
      --chain-id chain-local \
      --gas auto --gas-prices 1905nhash --gas-adjustment 2 \
      --yes

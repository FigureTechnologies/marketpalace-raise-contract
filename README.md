# Marketpalace Raise Contract

### build
1. make
2. make optimize

### store contract on chain
    provenanced -t tx wasm store ./artifacts/marketpalace_raise_contract.wasm \
      --home $NODE \
      --from validator \
      --chain-id $CHAIN \
      --gas auto --gas-prices 1905nhash --gas-adjustment 2 \
      --yes

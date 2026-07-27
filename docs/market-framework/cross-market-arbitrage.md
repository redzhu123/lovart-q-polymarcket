# Independent CEX and DEX arbitrage architecture

CEX and DEX share instrument identity, timestamps, metrics, storage and report
infrastructure. Their market data, strategy inputs and simulation semantics are
separate. This project does not construct CEX-to-DEX opportunities.

```text
CEX API -> CexMarketDataProvider -> VenueQuote -> CexArbitrageDetector
                                                   |
                                                   v
                                         CEX paper execution / replay

DEX RPC -> DexMarketDataProvider -> AmmPoolState -> DexPoolQuote
                                                   |
                                                   v
                                         DexArbitrageDetector
                                                   |
                                                   v
                                         AMM simulation / replay
```

## Shared identity

`CanonicalInstrument` identifies the economic product, for example spot
`BTC/USDT`. Equality allows storage and reporting to use one identity scheme,
but it does not make different execution domains comparable. `VenueKind` must
always be checked at a strategy boundary.

## CEX domain

`CexMarketDataProvider` publishes normalized `VenueQuote` values. Bid and ask
sizes are executable base-asset quantities. `CexArbitrageDetector` accepts only
quotes marked `VenueKind::Cex`, rejects stale data, subtracts taker fees and a
slippage buffer, and limits quantity by both books and configured notional.

Multiple CEX order gateways can be registered in `GatewayRouter`. This router
is for order-shaped execution and does not model AMM swaps.

## DEX domain

`DexMarketDataProvider` publishes `AmmPoolState` for replay and produces a
`DexPoolQuote` for a concrete base quantity. The executable quote includes LP
fees and price impact; network cost remains explicit in quote currency.

`DexArbitrageDetector` compares only DEX pool quotes for the same instrument and
quantity. It does not scale quotes linearly because AMM prices are nonlinear.
DEX simulation should replay pool or block state and request fresh executable
quotes for every tested quantity.

## Extension rule

Adding a CEX means implementing `CexMarketDataProvider`, a CEX adapter and an
order gateway. Adding a DEX means implementing `DexMarketDataProvider`, AMM
state decoding and a swap simulator. Neither path requires changes to the
other detector, and neither should depend on the prediction-market scanner.

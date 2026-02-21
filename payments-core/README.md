# payments-core

Core payment and ledger logic with **no I/O dependencies**. This crate implements account state, transaction processing, and dispute handling as pure functions so you can plug in any input source (CSV, streams, async, etc.) and any output format.

Part of the [payment-processor](../README.md) workspace.

## Features

- **Pure logic** — Only `rust_decimal` and `thiserror`; no files, no network, no async runtime.
- **Pluggable input** — Implement [`TransactionSource`] to feed transactions from CSV, APIs, or streams.
- **Type-safe IDs** — [`ClientId`] and [`TxId`] newtypes avoid mixing up clients and transaction IDs.
- **Dispute lifecycle** — Deposits follow a strict state machine: Active → Disputed → Resolved or ChargedBack.
- **4-decimal precision** — Amounts are normalized with banker's rounding (IEEE 754 MidpointNearestEven).

## Quick example

```rust
use payments_core::{ClientId, Ledger, Transaction, TxId, TxKind};
use rust_decimal_macros::dec;

let mut ledger = Ledger::new();

// Process a few transactions
ledger.process(Transaction {
    kind: TxKind::Deposit,
    client_id: ClientId(1),
    tx_id: TxId(1),
    amount: Some(dec!(100.50)),
}).ok();
ledger.process(Transaction {
    kind: TxKind::Withdrawal,
    client_id: ClientId(1),
    tx_id: TxId(2),
    amount: Some(dec!(25.25)),
}).ok();

// Read final state
for (client_id, account) in ledger.iter_accounts() {
    println!("client {}: available={}, held={}, locked={}",
        client_id, account.available, account.held, account.locked);
}
```

## Main types

| Type | Purpose |
|------|---------|
| [`Ledger`] | Central state: create with `Ledger::new()`, apply with `process()`, read with `iter_accounts()`, `iter_events()`. Replay via `from_events()`; audit via `from_accounts_and_events()`. |
| [`Event`] | A validated transaction that was applied; stored in the event log. **Amounts are unnormalized** (as received). Accessors: `kind()`, `client_id()`, `tx_id()`, `amount()`. |
| [`Transaction`] | One input event: `kind`, `client_id`, `tx_id`, optional `amount`. |
| [`Account`] | Per-client state: `available`, `held`, `locked`; `total()` = available + held. |
| [`TxKind`] | `Deposit`, `Withdrawal`, `Dispute`, `Resolve`, `Chargeback`. |
| [`TxState`] | Deposit lifecycle: `Active`, `Disputed`, `Resolved`, `ChargedBack`. |
| [`TransactionSource`] | Trait for iterating over `Result<Transaction, E>` (e.g. CSV parser). |
| [`LedgerError`] | Validation failures (locked account, insufficient funds, duplicate tx, etc.). |

## Transaction rules (summary)

- **Deposits** create or credit an account; amount required, positive.
- **Withdrawals** debit available funds; rejected if account missing or insufficient funds.
- **Disputes** hold the amount of a prior deposit; only active deposits can be disputed.
- **Resolve** releases held funds back to available (dispute resolved in client’s favor).
- **Chargeback** reverses the deposit and locks the account; no further transactions allowed.
- Amounts are normalized to 4 decimal places. Duplicate deposit `tx_id`s are rejected.

## Dependencies

- `rust_decimal` — decimal arithmetic (with `serde` for optional serialization).
- `thiserror` — `LedgerError` and `#[non_exhaustive]` enums.

For full usage (CLI, CSV I/O, examples), see the root [payment-processor](../README.md) crate.

# Payment Processor

![CI](https://github.com/zacharyfrederick/payment_processor/actions/workflows/ci.yml/badge.svg)
[![Rust](https://img.shields.io/badge/rust-stable-orange.svg)](https://www.rust-lang.org)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](LICENSE)

A simple transaction processor that reads a CSV of transactions and outputs the final state of client accounts.

<p align="center">
  <a href="#usage">Usage</a> &middot;
  <a href="#transaction-types">Transaction Types</a> &middot;
  <a href="#assumptions">Assumptions</a> &middot;
  <a href="#project-structure">Project Structure</a> &middot;
  <a href="#design-decisions">Design Decisions</a> &middot;
  <a href="#security">Security</a> &middot;
  <a href="#testing">Testing</a> &middot;
  <a href="#async-example">Async Example</a>
</p>

## AI Tools Used
- Claude (Web app)
- Cursor

## Usage

```shell
cargo run -- transactions.csv > accounts.csv
```

### Input Format

CSV with columns: `type`, `client`, `tx`, `amount`

```csv
type,client,tx,amount
deposit,1,1,1.0
deposit,2,2,2.0
withdrawal,1,3,1.5
dispute,1,1,
resolve,1,1,
```

### Output Format

CSV with columns: `client`, `available`, `held`, `total`, `locked`

```csv
client,available,held,total,locked
1,1.5000,0.0000,1.5000,false
2,2.0000,0.0000,2.0000,false
```

## Transaction Types

| Type | Description |
|------|-------------|
| `deposit` | Credit funds to the client's available balance |
| `withdrawal` | Debit funds from the client's available balance |
| `dispute` | Hold funds from a previous deposit pending investigation |
| `resolve` | Release held funds back to available (dispute resolved in client's favor) |
| `chargeback` | Reverse a disputed deposit and lock the account |

## Assumptions

The following assumptions were made where the specification was ambiguous:

### Decimal Precision

Amounts are normalized to 4 decimal places using **banker's rounding** (IEEE 754 MidpointNearestEven). This is standard practice in financial transaction processing to avoid systematic rounding bias across large transaction volumes.

Example: `1.23456789` becomes `1.2346`, `2.5` stays `2.5` (rounds to nearest even).

### Locked Accounts

When an account is locked (due to a chargeback), **all transactions are blocked** including deposits. This is a conservative assumption for fraud prevention — a locked account requires manual review before any further activity.

### Dispute Scope

**Only deposits can be disputed.** Withdrawals cannot be disputed through this system. In a real-world scenario, withdrawal disputes would involve the counterparty and require different mechanics (crediting rather than holding funds).

### Transaction State Machine

Each deposit follows a strict state machine:

```
Active → Disputed → Resolved (terminal)
                 → ChargedBack (terminal)
```

- Only `Active` deposits can be disputed
- Only `Disputed` deposits can be resolved or charged back
- `Resolved` and `ChargedBack` are terminal states — no further transitions allowed
- Attempting an invalid transition is a no-op (logged to stderr)

### Duplicate Transaction IDs

Transaction IDs must be globally unique. A deposit with a duplicate transaction ID is rejected as a no-op.

### Client/Transaction Mismatch

A dispute, resolve, or chargeback referencing a transaction that belongs to a different client is rejected as a no-op.

### Non-Existent Accounts

- Deposits create accounts automatically if they don't exist
- Withdrawals from non-existent accounts are rejected (insufficient funds)
- Disputes on non-existent accounts are rejected

### Error Handling

The processor is resilient — errors are logged to stderr but never fatal. Invalid transactions are skipped and processing continues. The program always produces output (even if empty).

## Project Structure

```
payment-processor/
├── Cargo.toml              # Workspace root + binary
├── payments-core/          # Core library (no I/O dependencies)
│   ├── Cargo.toml
│   └── src/
│       ├── lib.rs
│       ├── types.rs        # ClientId, TxId, Account, Transaction, etc.
│       ├── ledger.rs       # Ledger, process(), validate()
│       ├── error.rs        # LedgerError
│       └── source.rs       # TransactionSource trait
├── src/                    # Binary crate (CLI + I/O)
│   ├── main.rs             # CLI entrypoint
│   ├── lib.rs              # Args, run_sync, re-exports
│   ├── csv_source.rs       # CSV parser
│   ├── processor.rs        # Processor (wraps Ledger + source)
│   ├── error.rs            # ParseError
│   └── output.rs           # CSV output
├── examples/
│   └── async_bridge.rs     # Optional async example (--features async)
└── tests/
    ├── common/mod.rs       # Shared CSV normalization for comparison
    ├── golden.rs           # Full-pipeline golden test (golden.csv → expected.csv)
    ├── api_surface.rs      # CLI contract tests (args, missing file, output)
    ├── integration.rs      # Scenario tests (deposits, disputes, chargeback, etc.)
    └── fixtures/           # Test CSV files (basic, golden, expected, etc.)
```

## Design Decisions

### Pure Core Library

The `payments-core` crate has no I/O dependencies. All validation and state mutation logic is expressed as pure functions that are independently testable. The binary crate owns all I/O (CSV parsing, file reading, stdout writing).

### Type Safety

- `ClientId(u16)` and `TxId(u32)` newtypes prevent accidentally swapping IDs
- `TxState` enum with exhaustive matching ensures valid state transitions
- `total()` is a derived method, not a stored field, making the invariant `total == available + held` impossible to violate

### Validation/Apply Split

Transactions are processed in two phases:
1. `validate()` — checks all invariants, returns `Result<(), LedgerError>`
2. `apply_unchecked()` — performs the mutation, only called if validation passed

NO CHECKS ARE PERFORMED IN APPLY_UNCHECKED. THEY PURELY UPDATE STATE. We are okay with this arrangement because only `process` and `validate` are exposed publicly. So to update state you must call process which performs the necessary validations based on the Tx kind.


This separation allows the caller to handle errors (log and continue) without the core library needing to know about I/O.

## Security

### Dependencies

This project keeps dependencies minimal and uses well-known, trusted crates:

- **payments-core:** `rust_decimal` (decimal/financial math), `thiserror` (errors).
- **payment-processor:** `csv`, `serde`, `clap`; optional `tokio` for the async example.

[rust_decimal](https://crates.io/crates/rust_decimal) is the standard choice for decimal arithmetic in Rust and is widely used and maintained. All dependencies are from the official Rust ecosystem (crates.io) and have no known critical security issues at the time of writing. We run `cargo deny` in CI to check licenses and advisories.

### CI and public workflows

CI uses public GitHub Actions. Third-party actions can be updated by their maintainers, which introduces supply-chain risk. We pin each action to a **commit SHA** (immutable ref) so the workflow does not automatically pick up changes from the action repo.

| Action | SHA | Version / reason |
|--------|-----|------------------|
| `actions/checkout` | `34e114876b0b11c390a56381ad16ebd13914f8d5` | v4.3.1 — GitHub official |
| `dtolnay/rust-toolchain` | `e97e2d8cc328f1b50210efc529dca0028893a2d9` | v1 — David Tolnay |
| `Swatinem/rust-cache` | `779680da715d629ac1d338a641029a2f4372abb5` | v2.8.2 — Armin Ronacher / rust-cache |
| `EmbarkStudios/cargo-deny-action` | `3fd3802e88374d3fe9159b834c7714ec57d6c979` | v2.0.15 — Embark Studios (license/advisory checks) |
| `rustsec/audit-check` | `69366f33c96575abad1ee0dba8212993eecbe998` | v2.0.0 — [RustSec](https://github.com/rustsec) (security audit) |

These are maintained by well-known authors or organizations. Pinning to SHAs (rather than tags like `@v4` or `@v2`) ensures the exact revision is used until we explicitly update.

**rustsec/audit-check** is maintained by the [RustSec Project](https://rustsec.org/), which also maintains the [Rust Security Advisory Database](https://github.com/RustSec/advisory-db) and the [cargo-audit](https://github.com/RustSec/cargo-audit) tool. The action runs `cargo-audit` in CI to fail on known vulnerabilities. We trust it because RustSec is the canonical, community-backed source for Rust dependency advisories and the action is the official way to run that check in GitHub Actions.

## Testing

```shell
cargo test
```

This runs all unit tests (inside the library crates) and all integration tests (in `tests/`). Below is what each layer covers and why.

### Unit tests (library crates)

- **payments-core** — Core logic is heavily unit-tested so we can verify behavior without I/O:
  - **Ledger:** pure apply functions (deposit, withdrawal, dispute, resolve, chargeback), amount normalization, validation helpers (amount, sufficient funds, tx record, account unlocked), and overflow checks (validation layer rejects operations that would overflow/underflow). State-machine and invariant tests (e.g. locked account blocks deposit, duplicate tx rejected, total = available + held).
  - **Types / error:** display and parsing for IDs and account totals, error messages.
- **payment-processor (binary crate)** — CSV and wiring:
  - **csv_source:** parsing each transaction type, whitespace, invalid input, decimal precision.
  - **output:** CSV formatting, 4-decimal amounts, empty accounts.
  - **processor:** basic run and skipping invalid transactions.

These tests give fast, focused coverage of business rules and I/O boundaries.

### Integration tests (`tests/`)

Run against the compiled binary and fixture CSVs to assert end-to-end behavior and the public CLI contract.

| Test binary | What it does | Why |
|-------------|--------------|-----|
| **golden** | Runs the processor on `tests/fixtures/golden.csv` and compares stdout to `tests/fixtures/expected.csv` using normalized comparison (spacing, row order, and decimal formatting are ignored). | Locks in the full pipeline: same input must produce the same logical output so refactors don’t change behavior. |
| **api_surface** | **CLI contract:** (1) One argument (transactions file) runs successfully and stdout matches expected CSV (normalized). (2) No arguments exits with failure and an error on stderr. (3) Missing file path exits with failure and an error on stderr. | Ensures the documented usage (`cargo run -- transactions.csv > accounts.csv`) and error behavior stay stable. |
| **integration** | Scenario tests: basic deposits/withdrawals, dispute/resolve, chargeback and locked account, whitespace handling, decimal precision, and a comprehensive fixture. | Covers real-world flows and edge cases at the process boundary. |

Shared CSV normalization for golden and api_surface lives in `tests/common/mod.rs` so both compare output the same way (trim, normalize line endings, canonicalize decimals to 4 places, sort rows by client).

## Async Example

The `async_bridge` example simulates many concurrent streams (e.g. TCP connections) feeding a single ledger: each input file is read asynchronously, transactions are sent over a channel to one processor task, and the final accounts CSV is written when all streams have finished.

**Feature gating:** The async runtime (tokio) is optional so the default build stays dependency-light. Enable it with the `async` feature. Without it, the example binary still compiles but exits with instructions:

```shell
cargo run --example async_bridge --features async -- tests/fixtures/async_file1.csv tests/fixtures/async_file2.csv > accounts.csv
```

In `Cargo.toml`, `tokio` is an optional dependency and `async = ["dep:tokio"]`; only when you pass `--features async` is tokio built and the example’s async code included.

**Ordering:** The processor applies transactions in the order it receives them from the channel. Without a per-transaction sequence (e.g. a nonce or timestamp), there is no way to guarantee that transactions from different streams are applied in chronological order—they are effectively merged by channel arrival order. To preserve global order across streams you’d need to attach a sequence to each transaction and have the processor sort or buffer before applying.


//! Core payment and ledger logic with no I/O dependencies.
//!
//! The main entry point is [`Ledger`]: create with [`Ledger::new()`], apply transactions with
//! [`Ledger::process()`], and use [`Ledger::iter_accounts()`] to read state (e.g. for
//! serialization). This crate re-exports types ([`Account`], [`Transaction`], etc.), [`LedgerError`],
//! and the [`TransactionSource`] trait for pluggable input.
//!
//! # Example
//!
//! ```
//! use payments_core::{ClientId, Ledger, Transaction, TxId, TxKind};
//! use rust_decimal_macros::dec;
//!
//! let mut ledger = Ledger::new();
//!
//! ledger.process(Transaction {
//!     kind: TxKind::Deposit,
//!     client_id: ClientId(1),
//!     tx_id: TxId(1),
//!     amount: Some(dec!(100.50)),
//! }).ok();
//! ledger.process(Transaction {
//!     kind: TxKind::Withdrawal,
//!     client_id: ClientId(1),
//!     tx_id: TxId(2),
//!     amount: Some(dec!(25.25)),
//! }).ok();
//!
//! for (client_id, account) in ledger.iter_accounts() {
//!     println!("client {}: available={}, held={}, locked={}",
//!         client_id, account.available, account.held, account.locked);
//! }
//! ```

#![deny(clippy::all)]
#![deny(clippy::arithmetic_side_effects)] // flags unchecked math operations
#![deny(clippy::unwrap_used)] // forces proper error handling
#![deny(clippy::expect_used)] // same

pub mod error;
pub mod ledger;
pub mod source;
pub mod types;

pub use error::LedgerError;
pub use ledger::Ledger;
pub use source::TransactionSource;
pub use types::{Account, ClientId, Transaction, TxId, TxKind, TxRecord, TxState};

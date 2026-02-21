//! Core payment and ledger logic with no I/O dependencies.
//!
//! The main entry point is [`Ledger`]: create with [`Ledger::new()`], apply transactions with
//! [`Ledger::process()`], and use [`Ledger::iter_accounts()`] to read state (e.g. for
//! serialization). This crate re-exports types ([`Account`], [`Transaction`], etc.), [`LedgerError`],
//! and the [`TransactionSource`] trait for pluggable input.

pub mod error;
pub mod ledger;
pub mod source;
pub mod types;

pub use error::LedgerError;
pub use ledger::Ledger;
pub use source::TransactionSource;
pub use types::{Account, ClientId, Transaction, TxId, TxKind, TxRecord, TxState};

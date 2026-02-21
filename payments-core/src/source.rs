use crate::types::Transaction;
use std::error::Error;

/// A source of transactions to be processed by the ledger.
///
/// This trait abstracts over different input formats (CSV, streaming, etc.)
/// and follows the iterator pattern where each call returns the next transaction
/// or None when exhausted.
///
/// The associated `Error` type allows each implementation to define its own
/// parsing error type.
pub trait TransactionSource: Iterator<Item = Result<Transaction, Self::Error>> {
    /// The error type returned when parsing a transaction fails.
    type Error: Error;
}

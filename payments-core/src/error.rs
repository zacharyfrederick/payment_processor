use crate::types::{ClientId, TxId, TxState};
use thiserror::Error;

/// Error when the event log cannot durably record an event (e.g. write to Kafka/disk failed).
/// If this occurs, the transaction must not be applied to in-memory state (WAL contract).
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum EventLogError {
    #[error("event log write failed")]
    WriteFailed,
}

/// Errors that can occur when processing transactions against the ledger.
/// All errors represent no-op conditions - the transaction is rejected but processing continues.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum LedgerError {
    #[error(transparent)]
    EventLog(#[from] EventLogError),

    #[error("account is locked: client {0}")]
    AccountLocked(ClientId),

    #[error("insufficient funds: client {0}")]
    InsufficientFunds(ClientId),

    #[error("invalid amount: must be positive")]
    InvalidAmount,

    #[error("transaction not found: tx {0}")]
    TxNotFound(TxId),

    #[error("transaction belongs to different client")]
    TxClientMismatch,

    #[error("invalid state transition: tx {0} is in state {1}")]
    InvalidTxState(TxId, TxState),

    #[error("missing amount for transaction {0}")]
    MissingAmount(TxId),

    #[error("duplicate transaction id: tx {0}")]
    DuplicateTxId(TxId),

    #[error("arithmetic overflow or underflow")]
    Overflow,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_display() {
        let err = LedgerError::AccountLocked(ClientId(42));
        assert_eq!(err.to_string(), "account is locked: client 42");

        let err = LedgerError::InsufficientFunds(ClientId(1));
        assert_eq!(err.to_string(), "insufficient funds: client 1");

        let err = LedgerError::InvalidAmount;
        assert_eq!(err.to_string(), "invalid amount: must be positive");

        let err = LedgerError::TxNotFound(TxId(123));
        assert_eq!(err.to_string(), "transaction not found: tx 123");

        let err = LedgerError::InvalidTxState(TxId(1), TxState::Active);
        assert_eq!(
            err.to_string(),
            "invalid state transition: tx 1 is in state active"
        );

        let err = LedgerError::Overflow;
        assert_eq!(err.to_string(), "arithmetic overflow or underflow");
    }
}

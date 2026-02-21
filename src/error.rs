use thiserror::Error;

/// Errors that can occur when parsing transaction input.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum ParseError {
    #[error("CSV error: {0}")]
    Csv(#[from] csv::Error),

    #[error("invalid transaction type: {0}")]
    InvalidTxType(String),
}

use payments_core::{Ledger, Transaction};
use std::fmt::Display;

/// Transaction processor that combines a ledger with a transaction source.
///
/// Generic over `S` to support any transaction source (CSV, streaming, etc.)
/// and `E` for the source's error type.
pub struct Processor<S> {
    ledger: Ledger,
    source: S,
}

impl<S, E> Processor<S>
where
    S: Iterator<Item = Result<Transaction, E>>,
    E: Display,
{
    /// Creates a new processor with the given transaction source.
    pub fn new(source: S) -> Self {
        Self {
            ledger: Ledger::new(),
            source,
        }
    }

    /// Processes all transactions from the source and returns the final ledger state.
    ///
    /// Invalid transactions are logged to stderr and skipped.
    /// Parse errors from the source are also logged and skipped.
    pub fn run(mut self) -> Ledger {
        for (row, result) in self.source.enumerate() {
            match result {
                Ok(tx) => {
                    if let Err(e) = self.ledger.process(tx) {
                        eprintln!("warn: row {}: {e}", row + 2);
                    }
                }
                Err(e) => {
                    eprintln!("warn: row {}: {e}", row + 2);
                }
            }
        }
        self.ledger
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use payments_core::{ClientId, TxId, TxKind};
    use rust_decimal_macros::dec;
    use std::convert::Infallible;

    fn transactions_ok(
        txs: Vec<Transaction>,
    ) -> impl Iterator<Item = Result<Transaction, Infallible>> {
        txs.into_iter().map(Ok)
    }

    #[test]
    fn test_processor_basic() {
        let txs = vec![
            Transaction {
                kind: TxKind::Deposit,
                client_id: ClientId(1),
                tx_id: TxId(1),
                amount: Some(dec!(10.0)),
            },
            Transaction {
                kind: TxKind::Withdrawal,
                client_id: ClientId(1),
                tx_id: TxId(2),
                amount: Some(dec!(3.0)),
            },
        ];

        let ledger = Processor::new(transactions_ok(txs)).run();
        let account = ledger.iter_accounts().find(|(id, _)| **id == ClientId(1));
        assert!(account.is_some());

        let (_, account) = account.unwrap();
        assert_eq!(account.available, dec!(7.0));
    }

    #[test]
    fn test_processor_skips_invalid() {
        let txs = vec![
            Transaction {
                kind: TxKind::Deposit,
                client_id: ClientId(1),
                tx_id: TxId(1),
                amount: Some(dec!(5.0)),
            },
            Transaction {
                kind: TxKind::Withdrawal,
                client_id: ClientId(1),
                tx_id: TxId(2),
                amount: Some(dec!(10.0)), // insufficient funds
            },
        ];

        let ledger = Processor::new(transactions_ok(txs)).run();
        let (_, account) = ledger
            .iter_accounts()
            .find(|(id, _)| **id == ClientId(1))
            .unwrap();
        assert_eq!(account.available, dec!(5.0)); // withdrawal skipped
    }
}

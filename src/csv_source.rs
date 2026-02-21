use crate::error::ParseError;
use csv::{ReaderBuilder, Trim};
use payments_core::{ClientId, Transaction, TransactionSource, TxId, TxKind};
use rust_decimal::Decimal;
use serde::Deserialize;
use std::io::Read;
use std::str::FromStr;

/// Raw transaction record as deserialized from CSV.
/// Uses String for the type field to allow case-insensitive parsing.
#[derive(Debug, Deserialize)]
struct RawTransaction {
    #[serde(rename = "type")]
    kind: String,
    client: u16,
    tx: u32,
    #[serde(default, deserialize_with = "deserialize_optional_decimal")]
    amount: Option<Decimal>,
}

fn deserialize_optional_decimal<'de, D>(deserializer: D) -> Result<Option<Decimal>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let s: Option<String> = Option::deserialize(deserializer)?;
    match s {
        None => Ok(None),
        Some(s) if s.trim().is_empty() => Ok(None),
        Some(s) => Decimal::from_str(s.trim())
            .map(Some)
            .map_err(serde::de::Error::custom),
    }
}

/// A transaction source that reads from CSV input.
///
/// Generic over `R: Read` to support files, stdin, and in-memory buffers for testing.
pub struct CsvTransactionSource<R: Read> {
    reader: csv::Reader<R>,
}

impl<R: Read> CsvTransactionSource<R> {
    /// Creates a new CSV transaction source from a reader.
    pub fn new(reader: R) -> Self {
        Self {
            reader: ReaderBuilder::new()
                .trim(Trim::All)
                .flexible(true)
                .from_reader(reader),
        }
    }

    /// Converts a raw transaction record into a Transaction.
    fn convert(raw: RawTransaction) -> Result<Transaction, ParseError> {
        let kind =
            TxKind::from_str(&raw.kind).map_err(|e| ParseError::InvalidTxType(e.0.clone()))?;

        Ok(Transaction {
            kind,
            client_id: ClientId(raw.client),
            tx_id: TxId(raw.tx),
            amount: raw.amount,
        })
    }
}

impl<R: Read> Iterator for CsvTransactionSource<R> {
    type Item = Result<Transaction, ParseError>;

    fn next(&mut self) -> Option<Self::Item> {
        let mut record = csv::StringRecord::new();
        match self.reader.read_record(&mut record) {
            Ok(true) => match record.deserialize(self.reader.headers().ok()) {
                Ok(raw) => Some(Self::convert(raw)),
                Err(e) => Some(Err(ParseError::Csv(e))),
            },
            Ok(false) => None,
            Err(e) => Some(Err(ParseError::Csv(e))),
        }
    }
}

impl<R: Read> TransactionSource for CsvTransactionSource<R> {
    type Error = ParseError;
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal_macros::dec;
    use std::io::Cursor;

    fn source_from_str(s: &str) -> CsvTransactionSource<Cursor<&str>> {
        CsvTransactionSource::new(Cursor::new(s))
    }

    #[test]
    fn test_parse_deposit() {
        let csv = "type,client,tx,amount\ndeposit,1,1,1.5";
        let mut source = source_from_str(csv);

        let tx = source.next().unwrap().unwrap();
        assert_eq!(tx.kind, TxKind::Deposit);
        assert_eq!(tx.client_id, ClientId(1));
        assert_eq!(tx.tx_id, TxId(1));
        assert_eq!(tx.amount, Some(dec!(1.5)));
    }

    #[test]
    fn test_parse_withdrawal() {
        let csv = "type,client,tx,amount\nwithdrawal,2,3,2.0";
        let mut source = source_from_str(csv);

        let tx = source.next().unwrap().unwrap();
        assert_eq!(tx.kind, TxKind::Withdrawal);
        assert_eq!(tx.client_id, ClientId(2));
        assert_eq!(tx.tx_id, TxId(3));
        assert_eq!(tx.amount, Some(dec!(2.0)));
    }

    #[test]
    fn test_parse_dispute() {
        let csv = "type,client,tx,amount\ndispute,1,1,";
        let mut source = source_from_str(csv);

        let tx = source.next().unwrap().unwrap();
        assert_eq!(tx.kind, TxKind::Dispute);
        assert_eq!(tx.client_id, ClientId(1));
        assert_eq!(tx.tx_id, TxId(1));
        assert_eq!(tx.amount, None);
    }

    #[test]
    fn test_parse_resolve() {
        let csv = "type,client,tx,amount\nresolve,1,1";
        let mut source = source_from_str(csv);

        let tx = source.next().unwrap().unwrap();
        assert_eq!(tx.kind, TxKind::Resolve);
        assert_eq!(tx.amount, None);
    }

    #[test]
    fn test_parse_chargeback() {
        let csv = "type,client,tx,amount\nchargeback,1,1";
        let mut source = source_from_str(csv);

        let tx = source.next().unwrap().unwrap();
        assert_eq!(tx.kind, TxKind::Chargeback);
        assert_eq!(tx.amount, None);
    }

    #[test]
    fn test_whitespace_handling() {
        let csv = "type, client, tx, amount\n  deposit  ,  1  ,  1  ,  1.5  ";
        let mut source = source_from_str(csv);

        let tx = source.next().unwrap().unwrap();
        assert_eq!(tx.kind, TxKind::Deposit);
        assert_eq!(tx.client_id, ClientId(1));
        assert_eq!(tx.tx_id, TxId(1));
        assert_eq!(tx.amount, Some(dec!(1.5)));
    }

    #[test]
    fn test_multiple_transactions() {
        let csv = "type,client,tx,amount
deposit,1,1,1.0
deposit,2,2,2.0
withdrawal,1,3,0.5";
        let source = source_from_str(csv);

        let txs: Vec<_> = source.collect();
        assert_eq!(txs.len(), 3);
        assert!(txs.iter().all(|r| r.is_ok()));
    }

    #[test]
    fn test_invalid_type() {
        let csv = "type,client,tx,amount\ninvalid,1,1,1.0";
        let mut source = source_from_str(csv);

        let result = source.next().unwrap();
        assert!(matches!(result, Err(ParseError::InvalidTxType(_))));
    }

    #[test]
    fn test_invalid_client_id() {
        let csv = "type,client,tx,amount\ndeposit,abc,1,1.0";
        let mut source = source_from_str(csv);

        let result = source.next().unwrap();
        assert!(result.is_err());
    }

    #[test]
    fn test_invalid_amount() {
        let csv = "type,client,tx,amount\ndeposit,1,1,not_a_number";
        let mut source = source_from_str(csv);

        let result = source.next().unwrap();
        assert!(result.is_err());
    }

    #[test]
    fn test_empty_csv() {
        let csv = "type,client,tx,amount\n";
        let mut source = source_from_str(csv);

        assert!(source.next().is_none());
    }

    #[test]
    fn test_four_decimal_places() {
        let csv = "type,client,tx,amount\ndeposit,1,1,1.2345";
        let mut source = source_from_str(csv);

        let tx = source.next().unwrap().unwrap();
        assert_eq!(tx.amount, Some(dec!(1.2345)));
    }

    #[test]
    fn test_case_insensitive_type() {
        let csv = "type,client,tx,amount\nDEPOSIT,1,1,1.0\nWithdrawal,2,2,1.0";
        let mut source = source_from_str(csv);

        let tx1 = source.next().unwrap().unwrap();
        assert_eq!(tx1.kind, TxKind::Deposit);

        let tx2 = source.next().unwrap().unwrap();
        assert_eq!(tx2.kind, TxKind::Withdrawal);
    }
}

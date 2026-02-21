use rust_decimal::Decimal;
use std::fmt;
use std::str::FromStr;

/// Unique identifier for a client account.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ClientId(pub u16);

impl fmt::Display for ClientId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl FromStr for ClientId {
    type Err = std::num::ParseIntError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        s.trim().parse::<u16>().map(ClientId)
    }
}

/// Unique identifier for a transaction.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TxId(pub u32);

impl fmt::Display for TxId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl FromStr for TxId {
    type Err = std::num::ParseIntError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        s.trim().parse::<u32>().map(TxId)
    }
}

/// State of a deposit transaction in the dispute lifecycle.
///
/// State transitions:
/// - `Active` -> `Disputed` (via dispute)
/// - `Disputed` -> `Resolved` (via resolve, terminal)
/// - `Disputed` -> `ChargedBack` (via chargeback, terminal)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TxState {
    /// Transaction is in good standing, funds are available.
    Active,
    /// Transaction is under dispute, funds are held.
    Disputed,
    /// Dispute was resolved in favor of keeping the transaction, terminal state.
    Resolved,
    /// Dispute resulted in a chargeback, funds were reversed, terminal state.
    ChargedBack,
}

impl fmt::Display for TxState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TxState::Active => write!(f, "active"),
            TxState::Disputed => write!(f, "disputed"),
            TxState::Resolved => write!(f, "resolved"),
            TxState::ChargedBack => write!(f, "chargedback"),
        }
    }
}

/// Type of transaction being processed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum TxKind {
    /// Credit funds to the client's account.
    Deposit,
    /// Debit funds from the client's account.
    Withdrawal,
    /// Initiate a dispute on a previous deposit.
    Dispute,
    /// Resolve a dispute in favor of the original transaction.
    Resolve,
    /// Reverse a disputed transaction and lock the account.
    Chargeback,
}

impl fmt::Display for TxKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TxKind::Deposit => write!(f, "deposit"),
            TxKind::Withdrawal => write!(f, "withdrawal"),
            TxKind::Dispute => write!(f, "dispute"),
            TxKind::Resolve => write!(f, "resolve"),
            TxKind::Chargeback => write!(f, "chargeback"),
        }
    }
}

/// Error when parsing a transaction kind from a string.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseTxKindError(pub String);

impl fmt::Display for ParseTxKindError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "unknown transaction type: {}", self.0)
    }
}

impl std::error::Error for ParseTxKindError {}

impl FromStr for TxKind {
    type Err = ParseTxKindError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.trim().to_lowercase().as_str() {
            "deposit" => Ok(TxKind::Deposit),
            "withdrawal" => Ok(TxKind::Withdrawal),
            "dispute" => Ok(TxKind::Dispute),
            "resolve" => Ok(TxKind::Resolve),
            "chargeback" => Ok(TxKind::Chargeback),
            other => Err(ParseTxKindError(other.to_string())),
        }
    }
}

/// Record of a deposit transaction stored in the ledger.
/// Only deposits are stored because only deposits can be disputed.
#[derive(Debug, Clone)]
pub struct TxRecord {
    pub client_id: ClientId,
    pub amount: Decimal,
    pub state: TxState,
}

/// Client account holding funds.
#[derive(Debug, Clone)]
pub struct Account {
    /// Funds available for trading, staking, withdrawal, etc.
    pub available: Decimal,
    /// Funds held due to an ongoing dispute.
    pub held: Decimal,
    /// Whether the account is frozen due to a chargeback.
    pub locked: bool,
}

impl Account {
    /// Returns total funds (available + held).
    /// This is a derived value to enforce the invariant that total == available + held.
    #[must_use]
    pub fn total(&self) -> Decimal {
        self.available + self.held
    }
}

impl Default for Account {
    fn default() -> Self {
        Self {
            available: Decimal::ZERO,
            held: Decimal::ZERO,
            locked: false,
        }
    }
}

/// An incoming transaction to be processed by the ledger.
#[derive(Debug, Clone)]
pub struct Transaction {
    pub kind: TxKind,
    pub client_id: ClientId,
    pub tx_id: TxId,
    /// Amount for deposit/withdrawal transactions. None for dispute/resolve/chargeback.
    pub amount: Option<Decimal>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_client_id_display() {
        assert_eq!(ClientId(42).to_string(), "42");
    }

    #[test]
    fn test_client_id_from_str() {
        assert_eq!("42".parse::<ClientId>().unwrap(), ClientId(42));
        assert_eq!("  42  ".parse::<ClientId>().unwrap(), ClientId(42));
        assert!("invalid".parse::<ClientId>().is_err());
    }

    #[test]
    fn test_tx_id_display() {
        assert_eq!(TxId(123456).to_string(), "123456");
    }

    #[test]
    fn test_tx_id_from_str() {
        assert_eq!("123456".parse::<TxId>().unwrap(), TxId(123456));
        assert!("invalid".parse::<TxId>().is_err());
    }

    #[test]
    fn test_tx_kind_from_str() {
        assert_eq!("deposit".parse::<TxKind>().unwrap(), TxKind::Deposit);
        assert_eq!("DEPOSIT".parse::<TxKind>().unwrap(), TxKind::Deposit);
        assert_eq!("  Deposit  ".parse::<TxKind>().unwrap(), TxKind::Deposit);
        assert_eq!("withdrawal".parse::<TxKind>().unwrap(), TxKind::Withdrawal);
        assert_eq!("dispute".parse::<TxKind>().unwrap(), TxKind::Dispute);
        assert_eq!("resolve".parse::<TxKind>().unwrap(), TxKind::Resolve);
        assert_eq!("chargeback".parse::<TxKind>().unwrap(), TxKind::Chargeback);
        assert!("invalid".parse::<TxKind>().is_err());
    }

    #[test]
    fn test_account_total() {
        let account = Account {
            available: Decimal::new(100, 2),
            held: Decimal::new(50, 2),
            locked: false,
        };
        assert_eq!(account.total(), Decimal::new(150, 2));
    }

    #[test]
    fn test_account_default() {
        let account = Account::default();
        assert_eq!(account.available, Decimal::ZERO);
        assert_eq!(account.held, Decimal::ZERO);
        assert!(!account.locked);
    }
}

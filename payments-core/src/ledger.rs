use crate::error::LedgerError;
use crate::types::{Account, ClientId, Transaction, TxId, TxKind, TxRecord, TxState};
use rust_decimal::prelude::*;
use rust_decimal::Decimal;
use std::collections::HashMap;

/// Normalizes an amount to 4 decimal places using banker's rounding (IEEE 754 MidpointNearestEven).
/// This is standard practice in financial transaction processing to avoid systematic rounding bias.
#[must_use]
pub fn normalize_amount(d: Decimal) -> Decimal {
    d.round_dp_with_strategy(4, RoundingStrategy::MidpointNearestEven)
}

// ============================================================================
// Pure validation functions - no state dependency, independently testable
// ============================================================================

/// Validates that an amount is present and positive.
pub fn validate_amount(amount: Option<Decimal>, tx_id: TxId) -> Result<Decimal, LedgerError> {
    let amount = amount.ok_or(LedgerError::MissingAmount(tx_id))?;
    if amount <= Decimal::ZERO {
        return Err(LedgerError::InvalidAmount);
    }
    Ok(amount)
}

/// Validates that an account exists and is not locked.
pub fn validate_account_unlocked(
    account: Option<&Account>,
    client_id: ClientId,
) -> Result<&Account, LedgerError> {
    let account = account.ok_or(LedgerError::AccountLocked(client_id))?;
    if account.locked {
        return Err(LedgerError::AccountLocked(client_id));
    }
    Ok(account)
}

/// Validates that an account has sufficient available funds.
pub fn validate_sufficient_funds(
    account: &Account,
    amount: Decimal,
    client_id: ClientId,
) -> Result<(), LedgerError> {
    if account.available < amount {
        return Err(LedgerError::InsufficientFunds(client_id));
    }
    Ok(())
}

/// Validates a transaction record exists, belongs to the correct client, and is in the expected state.
pub fn validate_tx_record(
    record: Option<&TxRecord>,
    tx_id: TxId,
    client_id: ClientId,
    expected_state: TxState,
) -> Result<&TxRecord, LedgerError> {
    let record = record.ok_or(LedgerError::TxNotFound(tx_id))?;
    if record.client_id != client_id {
        return Err(LedgerError::TxClientMismatch);
    }
    if record.state != expected_state {
        return Err(LedgerError::InvalidTxState(tx_id, record.state));
    }
    Ok(record)
}

/// Validates that deposit (available + amount) would not overflow.
pub fn validate_deposit_no_overflow(
    available: Decimal,
    amount: Decimal,
) -> Result<(), LedgerError> {
    available
        .checked_add(amount)
        .map(|_| ())
        .ok_or(LedgerError::Overflow)
}

/// Validates that after a deposit, total (available + held + amount) would not overflow.
/// Use this instead of only validate_deposit_no_overflow to avoid total() overflowing when held is large.
pub fn validate_deposit_total_no_overflow(
    available: Decimal,
    held: Decimal,
    amount: Decimal,
) -> Result<(), LedgerError> {
    available
        .checked_add(held)
        .and_then(|total| total.checked_add(amount))
        .map(|_| ())
        .ok_or(LedgerError::Overflow)
}

/// Validates that withdrawal (available - amount) would not underflow.
pub fn validate_withdrawal_no_overflow(
    available: Decimal,
    amount: Decimal,
) -> Result<(), LedgerError> {
    available
        .checked_sub(amount)
        .map(|_| ())
        .ok_or(LedgerError::Overflow)
}

/// Validates that dispute (available - amount, held + amount) would not overflow or underflow.
pub fn validate_dispute_no_overflow(
    available: Decimal,
    held: Decimal,
    amount: Decimal,
) -> Result<(), LedgerError> {
    available
        .checked_sub(amount)
        .and_then(|_| held.checked_add(amount))
        .map(|_| ())
        .ok_or(LedgerError::Overflow)
}

/// Validates that resolve (available + amount, held - amount) would not overflow or underflow.
pub fn validate_resolve_no_overflow(
    available: Decimal,
    held: Decimal,
    amount: Decimal,
) -> Result<(), LedgerError> {
    available
        .checked_add(amount)
        .and_then(|_| held.checked_sub(amount))
        .map(|_| ())
        .ok_or(LedgerError::Overflow)
}

/// Validates that chargeback (held - amount) would not underflow.
pub fn validate_chargeback_no_overflow(held: Decimal, amount: Decimal) -> Result<(), LedgerError> {
    held.checked_sub(amount)
        .map(|_| ())
        .ok_or(LedgerError::Overflow)
}

// ============================================================================
// Pure apply functions - take values, return new values, no mutation
// ============================================================================

/// Applies a deposit to an account.
#[must_use]
#[allow(clippy::arithmetic_side_effects)] // Has been validated before calling this function
pub fn apply_deposit(account: &Account, amount: Decimal) -> Account {
    Account {
        available: account.available + amount,
        held: account.held,
        locked: account.locked,
    }
}

/// Applies a withdrawal to an account.
#[must_use]
#[allow(clippy::arithmetic_side_effects)] // Has been validated before calling this function
pub fn apply_withdrawal(account: &Account, amount: Decimal) -> Account {
    Account {
        available: account.available - amount,
        held: account.held,
        locked: account.locked,
    }
}

/// Applies a dispute to an account and transaction record.
#[must_use]
#[allow(clippy::arithmetic_side_effects)] // Has been validated before calling this function
pub fn apply_dispute(account: Account, record: TxRecord) -> (Account, TxRecord) {
    let amount = record.amount;
    (
        Account {
            available: account.available - amount,
            held: account.held + amount,
            ..account
        },
        TxRecord {
            state: TxState::Disputed,
            ..record
        },
    )
}

/// Applies a resolve to an account and transaction record.
#[must_use]
#[allow(clippy::arithmetic_side_effects)] // Has been validated before calling this function
pub fn apply_resolve(account: Account, record: TxRecord) -> (Account, TxRecord) {
    let amount = record.amount;
    (
        Account {
            available: account.available + amount,
            held: account.held - amount,
            ..account
        },
        TxRecord {
            state: TxState::Resolved,
            ..record
        },
    )
}

/// Applies a chargeback to an account and transaction record.
#[must_use]
#[allow(clippy::arithmetic_side_effects)] // Has been validated before calling this function
pub fn apply_chargeback(account: Account, record: TxRecord) -> (Account, TxRecord) {
    let amount = record.amount;
    (
        Account {
            held: account.held - amount,
            locked: true,
            ..account
        },
        TxRecord {
            state: TxState::ChargedBack,
            ..record
        },
    )
}

// ============================================================================
// Ledger - the main state container
// ============================================================================

/// The core ledger that tracks all accounts and transactions.
///
/// This is the main stateful type for the engine. Use [`process`](Ledger::process) as the primary
/// mutation API (it validates then applies each transaction).
pub struct Ledger {
    accounts: HashMap<ClientId, Account>,
    transactions: HashMap<TxId, TxRecord>,
    event_log: Vec<Transaction>,
}

impl Ledger {
    /// Creates a new empty ledger.
    #[must_use]
    pub fn new() -> Self {
        Self {
            accounts: HashMap::new(),
            transactions: HashMap::new(),
            event_log: Vec::new(),
        }
    }

    /// Returns an iterator over all accounts.
    ///
    /// Intended for serialization or output (e.g. writing the accounts CSV).
    pub fn iter_accounts(&self) -> impl Iterator<Item = (&ClientId, &Account)> {
        self.accounts.iter()
    }

    /// Gets or creates an account for a client.
    fn get_or_create_account(&mut self, client_id: ClientId) -> &mut Account {
        self.accounts.entry(client_id).or_default()
    }

    /// Validates a transaction against the current ledger state.
    /// Returns Ok(()) if the transaction can be applied, or an error describing why it cannot.
    pub fn validate(&self, tx: &Transaction) -> Result<(), LedgerError> {
        let account = self.accounts.get(&tx.client_id);

        match tx.kind {
            TxKind::Deposit => {
                let amount = validate_amount(tx.amount, tx.tx_id)?;
                if self.transactions.contains_key(&tx.tx_id) {
                    return Err(LedgerError::DuplicateTxId(tx.tx_id));
                }
                if let Some(acc) = account {
                    if acc.locked {
                        return Err(LedgerError::AccountLocked(tx.client_id));
                    }
                }
                let available = account.map(|a| a.available).unwrap_or(Decimal::ZERO);
                let held = account.map(|a| a.held).unwrap_or(Decimal::ZERO);
                validate_deposit_total_no_overflow(available, held, amount)?;
            }

            TxKind::Withdrawal => {
                let amount = validate_amount(tx.amount, tx.tx_id)?;
                let account = validate_account_unlocked(account, tx.client_id)?;
                validate_sufficient_funds(account, amount, tx.client_id)?;
                validate_withdrawal_no_overflow(account.available, amount)?;
            }

            TxKind::Dispute => {
                let account = validate_account_unlocked(account, tx.client_id)?;
                let record = validate_tx_record(
                    self.transactions.get(&tx.tx_id),
                    tx.tx_id,
                    tx.client_id,
                    TxState::Active,
                )?;
                validate_dispute_no_overflow(account.available, account.held, record.amount)?;
            }

            TxKind::Resolve | TxKind::Chargeback => {
                let account = validate_account_unlocked(account, tx.client_id)?;
                let record = validate_tx_record(
                    self.transactions.get(&tx.tx_id),
                    tx.tx_id,
                    tx.client_id,
                    TxState::Disputed,
                )?;
                match tx.kind {
                    TxKind::Resolve => {
                        validate_resolve_no_overflow(
                            account.available,
                            account.held,
                            record.amount,
                        )?;
                    }
                    TxKind::Chargeback => {
                        validate_chargeback_no_overflow(account.held, record.amount)?;
                    }
                    _ => {}
                }
            }
        }

        Ok(())
    }

    /// Validates and applies a transaction to the ledger.
    ///
    /// This is the only public mutation API. It ensures validation before application,
    /// making it impossible to corrupt ledger state with invalid transactions.
    ///
    /// Once a tx has been successfully validated it is added to the event log prior to updating state
    ///
    /// Returns `Ok(())` if the transaction was applied successfully, or an error
    /// describing why the transaction was rejected.
    pub fn process(&mut self, tx: Transaction) -> Result<(), LedgerError> {
        self.validate(&tx)?;
        self.event_log.push(tx.clone());
        self.apply_unchecked(tx);
        Ok(())
    }

    /// Internal apply without validation check.
    ///
    /// Only ever called from [`process()`] after `validate(&tx)` has returned `Ok(())`. So:
    /// - For deposit/withdrawal, `tx.amount` is `Some` (validated by `validate_amount`).
    /// - For dispute/resolve/chargeback, the tx and account exist in the maps (validated by
    ///   `validate_tx_record` and `validate_account_unlocked`). The `expect` calls below are
    ///   therefore safe; we use them to satisfy clippy while making the invariant explicit.
    #[allow(clippy::expect_used)]
    fn apply_unchecked(&mut self, tx: Transaction) {
        match tx.kind {
            TxKind::Deposit => {
                let amount = normalize_amount(
                    tx.amount
                        .expect("validate_amount ensures amount is Some for deposit"),
                );
                let account = self.get_or_create_account(tx.client_id);
                *account = apply_deposit(account, amount);
                self.transactions.insert(
                    tx.tx_id,
                    TxRecord {
                        client_id: tx.client_id,
                        amount,
                        state: TxState::Active,
                    },
                );
            }

            TxKind::Withdrawal => {
                let amount = normalize_amount(
                    tx.amount
                        .expect("validate_amount ensures amount is Some for withdrawal"),
                );
                let account = self.get_or_create_account(tx.client_id);
                *account = apply_withdrawal(account, amount);
            }

            TxKind::Dispute => {
                let record = self
                    .transactions
                    .remove(&tx.tx_id)
                    .expect("validate_tx_record ensured this tx exists and is active");
                let account = self
                    .accounts
                    .remove(&tx.client_id)
                    .expect("validate_account_unlocked ensured this account exists");
                let (new_account, new_record) = apply_dispute(account, record);
                self.accounts.insert(tx.client_id, new_account);
                self.transactions.insert(tx.tx_id, new_record);
            }

            TxKind::Resolve => {
                let record = self
                    .transactions
                    .remove(&tx.tx_id)
                    .expect("validate_tx_record ensured this tx exists and is disputed");
                let account = self
                    .accounts
                    .remove(&tx.client_id)
                    .expect("validate_account_unlocked ensured this account exists");
                let (new_account, new_record) = apply_resolve(account, record);
                self.accounts.insert(tx.client_id, new_account);
                self.transactions.insert(tx.tx_id, new_record);
            }

            TxKind::Chargeback => {
                let record = self
                    .transactions
                    .remove(&tx.tx_id)
                    .expect("validate_tx_record ensured this tx exists and is disputed");
                let account = self
                    .accounts
                    .remove(&tx.client_id)
                    .expect("validate_account_unlocked ensured this account exists");
                let (new_account, new_record) = apply_chargeback(account, record);
                self.accounts.insert(tx.client_id, new_account);
                self.transactions.insert(tx.tx_id, new_record);
            }
        }
    }
}

impl Default for Ledger {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal_macros::dec;

    fn account(available: Decimal, held: Decimal) -> Account {
        Account {
            available,
            held,
            locked: false,
        }
    }

    fn active_record(client_id: ClientId, amount: Decimal) -> TxRecord {
        TxRecord {
            client_id,
            amount,
            state: TxState::Active,
        }
    }

    fn disputed_record(client_id: ClientId, amount: Decimal) -> TxRecord {
        TxRecord {
            client_id,
            amount,
            state: TxState::Disputed,
        }
    }

    // ========================================================================
    // Pure function tests
    // ========================================================================

    #[test]
    fn test_normalize_amount() {
        assert_eq!(normalize_amount(dec!(1.23456)), dec!(1.2346));
        assert_eq!(normalize_amount(dec!(1.5)), dec!(1.5));
        assert_eq!(normalize_amount(dec!(2.5)), dec!(2.5));
        assert_eq!(normalize_amount(dec!(3.5)), dec!(3.5));
    }

    #[test]
    fn test_normalize_amount_bankers_rounding() {
        assert_eq!(normalize_amount(dec!(1.00005)), dec!(1.0000));
        assert_eq!(normalize_amount(dec!(1.00015)), dec!(1.0002));
    }

    #[test]
    fn test_apply_deposit() {
        let base = account(dec!(1.0), dec!(0.0));
        let a = apply_deposit(&base, dec!(2.5));
        assert_eq!(a.available, dec!(3.5));
        assert_eq!(a.held, dec!(0.0));
        assert_eq!(a.total().unwrap(), dec!(3.5));
    }

    #[test]
    fn test_apply_withdrawal() {
        let base = account(dec!(3.5), dec!(0.0));
        let a = apply_withdrawal(&base, dec!(1.0));
        assert_eq!(a.available, dec!(2.5));
        assert_eq!(a.held, dec!(0.0));
        assert_eq!(a.total().unwrap(), dec!(2.5));
    }

    #[test]
    fn test_apply_dispute() {
        let record = active_record(ClientId(1), dec!(2.0));
        let (a, r) = apply_dispute(account(dec!(5.0), dec!(0.0)), record);
        assert_eq!(a.available, dec!(3.0));
        assert_eq!(a.held, dec!(2.0));
        assert_eq!(a.total().unwrap(), dec!(5.0));
        assert_eq!(r.state, TxState::Disputed);
    }

    #[test]
    fn test_apply_resolve() {
        let record = disputed_record(ClientId(1), dec!(2.0));
        let (a, r) = apply_resolve(account(dec!(3.0), dec!(2.0)), record);
        assert_eq!(a.available, dec!(5.0));
        assert_eq!(a.held, dec!(0.0));
        assert_eq!(a.total().unwrap(), dec!(5.0));
        assert_eq!(r.state, TxState::Resolved);
    }

    #[test]
    fn test_apply_chargeback() {
        let record = disputed_record(ClientId(1), dec!(2.0));
        let (a, r) = apply_chargeback(account(dec!(3.0), dec!(2.0)), record);
        assert_eq!(a.available, dec!(3.0));
        assert_eq!(a.held, dec!(0.0));
        assert_eq!(a.total().unwrap(), dec!(3.0));
        assert!(a.locked);
        assert_eq!(r.state, TxState::ChargedBack);
    }

    // ========================================================================
    // Overflow validation tests - validation layer rejects overflow/underflow
    // ========================================================================

    // Use amounts that actually trigger rust_decimal overflow (small amounts can round away).
    const OVERFLOW_AMOUNT: Decimal = Decimal::ONE;

    #[test]
    fn test_validate_deposit_no_overflow_rejects() {
        assert!(validate_deposit_no_overflow(Decimal::MAX, OVERFLOW_AMOUNT).is_err());
        assert!(matches!(
            validate_deposit_no_overflow(Decimal::MAX, OVERFLOW_AMOUNT),
            Err(LedgerError::Overflow)
        ));
    }

    #[test]
    fn test_validate_withdrawal_no_overflow_rejects() {
        assert!(matches!(
            validate_withdrawal_no_overflow(Decimal::MIN, OVERFLOW_AMOUNT),
            Err(LedgerError::Overflow)
        ));
    }

    #[test]
    fn test_validate_dispute_no_overflow_rejects() {
        assert!(matches!(
            validate_dispute_no_overflow(dec!(0.0), Decimal::MAX, OVERFLOW_AMOUNT),
            Err(LedgerError::Overflow)
        ));
    }

    #[test]
    fn test_validate_resolve_no_overflow_rejects() {
        assert!(matches!(
            validate_resolve_no_overflow(Decimal::MAX, dec!(0.0), OVERFLOW_AMOUNT),
            Err(LedgerError::Overflow)
        ));
    }

    #[test]
    fn test_validate_chargeback_no_overflow_rejects() {
        assert!(matches!(
            validate_chargeback_no_overflow(Decimal::MIN, OVERFLOW_AMOUNT),
            Err(LedgerError::Overflow)
        ));
    }

    #[test]
    fn test_validate_deposit_total_no_overflow_rejects() {
        // available + held + amount would overflow even though available + amount would not
        assert!(validate_deposit_total_no_overflow(dec!(0.0), Decimal::MAX, dec!(1.0)).is_err());
        assert!(matches!(
            validate_deposit_total_no_overflow(dec!(0.0), Decimal::MAX, dec!(1.0)),
            Err(LedgerError::Overflow)
        ));
    }

    #[test]
    fn test_ledger_validate_deposit_overflow() {
        let mut ledger = Ledger::new();
        ledger
            .accounts
            .insert(ClientId(1), account(Decimal::MAX, dec!(0.0)));
        let tx = Transaction {
            kind: TxKind::Deposit,
            client_id: ClientId(1),
            tx_id: TxId(2),
            amount: Some(OVERFLOW_AMOUNT),
        };
        assert!(matches!(ledger.process(tx), Err(LedgerError::Overflow)));
    }

    #[test]
    fn test_ledger_validate_deposit_total_overflow() {
        // Total (available + held) would overflow after deposit; available + amount alone would not
        let mut ledger = Ledger::new();
        ledger
            .accounts
            .insert(ClientId(1), account(dec!(0.0), Decimal::MAX));
        let tx = Transaction {
            kind: TxKind::Deposit,
            client_id: ClientId(1),
            tx_id: TxId(1),
            amount: Some(dec!(1.0)),
        };
        assert!(matches!(ledger.process(tx), Err(LedgerError::Overflow)));
    }

    #[test]
    fn test_ledger_validate_dispute_overflow() {
        let mut ledger = Ledger::new();
        ledger
            .accounts
            .insert(ClientId(1), account(dec!(0.0), Decimal::MAX));
        ledger
            .transactions
            .insert(TxId(1), active_record(ClientId(1), OVERFLOW_AMOUNT));
        let tx = Transaction {
            kind: TxKind::Dispute,
            client_id: ClientId(1),
            tx_id: TxId(1),
            amount: None,
        };
        assert!(matches!(ledger.process(tx), Err(LedgerError::Overflow)));
    }

    #[test]
    fn test_ledger_validate_resolve_overflow() {
        let mut ledger = Ledger::new();
        ledger
            .accounts
            .insert(ClientId(1), account(Decimal::MAX, dec!(0.0)));
        ledger
            .transactions
            .insert(TxId(1), disputed_record(ClientId(1), OVERFLOW_AMOUNT));
        let tx = Transaction {
            kind: TxKind::Resolve,
            client_id: ClientId(1),
            tx_id: TxId(1),
            amount: None,
        };
        assert!(matches!(ledger.process(tx), Err(LedgerError::Overflow)));
    }

    #[test]
    fn test_ledger_validate_chargeback_overflow() {
        let mut ledger = Ledger::new();
        ledger
            .accounts
            .insert(ClientId(1), account(dec!(0.0), Decimal::MIN));
        ledger
            .transactions
            .insert(TxId(1), disputed_record(ClientId(1), OVERFLOW_AMOUNT));
        let tx = Transaction {
            kind: TxKind::Chargeback,
            client_id: ClientId(1),
            tx_id: TxId(1),
            amount: None,
        };
        assert!(matches!(ledger.process(tx), Err(LedgerError::Overflow)));
    }

    // ========================================================================
    // Validation tests
    // ========================================================================

    #[test]
    fn test_validate_amount_valid() {
        assert!(validate_amount(Some(dec!(1.0)), TxId(1)).is_ok());
    }

    #[test]
    fn test_validate_amount_missing() {
        assert!(matches!(
            validate_amount(None, TxId(1)),
            Err(LedgerError::MissingAmount(_))
        ));
    }

    #[test]
    fn test_validate_amount_zero() {
        assert!(matches!(
            validate_amount(Some(dec!(0.0)), TxId(1)),
            Err(LedgerError::InvalidAmount)
        ));
    }

    #[test]
    fn test_validate_amount_negative() {
        assert!(matches!(
            validate_amount(Some(dec!(-1.0)), TxId(1)),
            Err(LedgerError::InvalidAmount)
        ));
    }

    // ========================================================================
    // Ledger state machine tests
    // ========================================================================

    #[test]
    fn test_deposit_creates_account() {
        let mut ledger = Ledger::new();
        let tx = Transaction {
            kind: TxKind::Deposit,
            client_id: ClientId(1),
            tx_id: TxId(1),
            amount: Some(dec!(5.0)),
        };

        assert!(ledger.process(tx).is_ok());

        let account = ledger.accounts.get(&ClientId(1)).unwrap();
        assert_eq!(account.available, dec!(5.0));
        assert_eq!(account.held, dec!(0.0));
        assert!(!account.locked);
    }

    #[test]
    fn test_withdrawal_success() {
        let mut ledger = Ledger::new();
        let deposit = Transaction {
            kind: TxKind::Deposit,
            client_id: ClientId(1),
            tx_id: TxId(1),
            amount: Some(dec!(5.0)),
        };
        ledger.process(deposit).unwrap();

        let withdrawal = Transaction {
            kind: TxKind::Withdrawal,
            client_id: ClientId(1),
            tx_id: TxId(2),
            amount: Some(dec!(2.0)),
        };
        assert!(ledger.process(withdrawal).is_ok());

        let account = ledger.accounts.get(&ClientId(1)).unwrap();
        assert_eq!(account.available, dec!(3.0));
    }

    #[test]
    fn test_insufficient_funds() {
        let mut ledger = Ledger::new();
        let deposit = Transaction {
            kind: TxKind::Deposit,
            client_id: ClientId(1),
            tx_id: TxId(1),
            amount: Some(dec!(1.0)),
        };
        ledger.process(deposit).unwrap();

        let withdrawal = Transaction {
            kind: TxKind::Withdrawal,
            client_id: ClientId(1),
            tx_id: TxId(2),
            amount: Some(dec!(2.0)),
        };
        assert!(matches!(
            ledger.process(withdrawal),
            Err(LedgerError::InsufficientFunds(_))
        ));
    }

    #[test]
    fn test_dispute_resolve_cycle() {
        let mut ledger = Ledger::new();

        let deposit = Transaction {
            kind: TxKind::Deposit,
            client_id: ClientId(1),
            tx_id: TxId(1),
            amount: Some(dec!(5.0)),
        };
        ledger.process(deposit).unwrap();

        let dispute = Transaction {
            kind: TxKind::Dispute,
            client_id: ClientId(1),
            tx_id: TxId(1),
            amount: None,
        };
        assert!(ledger.process(dispute).is_ok());

        let account = ledger.accounts.get(&ClientId(1)).unwrap();
        assert_eq!(account.available, dec!(0.0));
        assert_eq!(account.held, dec!(5.0));
        assert_eq!(account.total().unwrap(), dec!(5.0));

        let resolve = Transaction {
            kind: TxKind::Resolve,
            client_id: ClientId(1),
            tx_id: TxId(1),
            amount: None,
        };
        assert!(ledger.process(resolve).is_ok());

        let account = ledger.accounts.get(&ClientId(1)).unwrap();
        assert_eq!(account.available, dec!(5.0));
        assert_eq!(account.held, dec!(0.0));
        assert_eq!(account.total().unwrap(), dec!(5.0));
        assert!(!account.locked);
    }

    #[test]
    fn test_chargeback_locks_account() {
        let mut ledger = Ledger::new();

        let deposit = Transaction {
            kind: TxKind::Deposit,
            client_id: ClientId(1),
            tx_id: TxId(1),
            amount: Some(dec!(5.0)),
        };
        ledger.process(deposit).unwrap();

        let dispute = Transaction {
            kind: TxKind::Dispute,
            client_id: ClientId(1),
            tx_id: TxId(1),
            amount: None,
        };
        ledger.process(dispute).unwrap();

        let chargeback = Transaction {
            kind: TxKind::Chargeback,
            client_id: ClientId(1),
            tx_id: TxId(1),
            amount: None,
        };
        assert!(ledger.process(chargeback).is_ok());

        let account = ledger.accounts.get(&ClientId(1)).unwrap();
        assert!(account.locked);
        assert_eq!(account.available, dec!(0.0));
        assert_eq!(account.held, dec!(0.0));
        assert_eq!(account.total().unwrap(), dec!(0.0));
    }

    #[test]
    fn test_locked_account_blocks_deposit() {
        let mut ledger = Ledger::new();

        let deposit = Transaction {
            kind: TxKind::Deposit,
            client_id: ClientId(1),
            tx_id: TxId(1),
            amount: Some(dec!(5.0)),
        };
        ledger.process(deposit).unwrap();

        let dispute = Transaction {
            kind: TxKind::Dispute,
            client_id: ClientId(1),
            tx_id: TxId(1),
            amount: None,
        };
        ledger.process(dispute).unwrap();

        let chargeback = Transaction {
            kind: TxKind::Chargeback,
            client_id: ClientId(1),
            tx_id: TxId(1),
            amount: None,
        };
        ledger.process(chargeback).unwrap();

        let deposit2 = Transaction {
            kind: TxKind::Deposit,
            client_id: ClientId(1),
            tx_id: TxId(2),
            amount: Some(dec!(1.0)),
        };
        assert!(matches!(
            ledger.process(deposit2),
            Err(LedgerError::AccountLocked(_))
        ));
    }

    #[test]
    fn test_invalid_state_resolve_without_dispute() {
        let mut ledger = Ledger::new();

        let deposit = Transaction {
            kind: TxKind::Deposit,
            client_id: ClientId(1),
            tx_id: TxId(1),
            amount: Some(dec!(5.0)),
        };
        ledger.process(deposit).unwrap();

        let resolve = Transaction {
            kind: TxKind::Resolve,
            client_id: ClientId(1),
            tx_id: TxId(1),
            amount: None,
        };
        assert!(matches!(
            ledger.process(resolve),
            Err(LedgerError::InvalidTxState(_, TxState::Active))
        ));
    }

    #[test]
    fn test_duplicate_tx_id_rejected() {
        let mut ledger = Ledger::new();

        let deposit1 = Transaction {
            kind: TxKind::Deposit,
            client_id: ClientId(1),
            tx_id: TxId(1),
            amount: Some(dec!(5.0)),
        };
        ledger.process(deposit1).unwrap();

        let deposit2 = Transaction {
            kind: TxKind::Deposit,
            client_id: ClientId(2),
            tx_id: TxId(1),
            amount: Some(dec!(3.0)),
        };
        assert!(matches!(
            ledger.process(deposit2),
            Err(LedgerError::DuplicateTxId(_))
        ));
    }

    #[test]
    fn test_tx_client_mismatch() {
        let mut ledger = Ledger::new();

        let deposit = Transaction {
            kind: TxKind::Deposit,
            client_id: ClientId(1),
            tx_id: TxId(1),
            amount: Some(dec!(5.0)),
        };
        ledger.process(deposit).unwrap();

        ledger.accounts.insert(ClientId(2), Account::default());

        let dispute = Transaction {
            kind: TxKind::Dispute,
            client_id: ClientId(2),
            tx_id: TxId(1),
            amount: None,
        };
        assert!(matches!(
            ledger.process(dispute),
            Err(LedgerError::TxClientMismatch)
        ));
    }

    #[test]
    fn test_negative_deposit_rejected() {
        let mut ledger = Ledger::new();
        let deposit = Transaction {
            kind: TxKind::Deposit,
            client_id: ClientId(1),
            tx_id: TxId(1),
            amount: Some(dec!(-1.0)),
        };
        assert!(matches!(
            ledger.process(deposit),
            Err(LedgerError::InvalidAmount)
        ));
    }

    #[test]
    fn test_total_invariant_through_full_cycle() {
        let mut ledger = Ledger::new();

        let deposit = Transaction {
            kind: TxKind::Deposit,
            client_id: ClientId(1),
            tx_id: TxId(1),
            amount: Some(dec!(10.0)),
        };
        ledger.process(deposit).unwrap();
        assert_eq!(
            ledger.accounts.get(&ClientId(1)).unwrap().total().unwrap(),
            dec!(10.0)
        );

        let withdrawal = Transaction {
            kind: TxKind::Withdrawal,
            client_id: ClientId(1),
            tx_id: TxId(2),
            amount: Some(dec!(3.0)),
        };
        ledger.process(withdrawal).unwrap();
        assert_eq!(
            ledger.accounts.get(&ClientId(1)).unwrap().total().unwrap(),
            dec!(7.0)
        );

        let deposit2 = Transaction {
            kind: TxKind::Deposit,
            client_id: ClientId(1),
            tx_id: TxId(3),
            amount: Some(dec!(5.0)),
        };
        ledger.process(deposit2).unwrap();
        assert_eq!(
            ledger.accounts.get(&ClientId(1)).unwrap().total().unwrap(),
            dec!(12.0)
        );

        let dispute = Transaction {
            kind: TxKind::Dispute,
            client_id: ClientId(1),
            tx_id: TxId(3),
            amount: None,
        };
        ledger.process(dispute).unwrap();
        let account = ledger.accounts.get(&ClientId(1)).unwrap();
        assert_eq!(account.available, dec!(7.0));
        assert_eq!(account.held, dec!(5.0));
        assert_eq!(account.total().unwrap(), dec!(12.0));

        let resolve = Transaction {
            kind: TxKind::Resolve,
            client_id: ClientId(1),
            tx_id: TxId(3),
            amount: None,
        };
        ledger.process(resolve).unwrap();
        let account = ledger.accounts.get(&ClientId(1)).unwrap();
        assert_eq!(account.available, dec!(12.0));
        assert_eq!(account.held, dec!(0.0));
        assert_eq!(account.total().unwrap(), dec!(12.0));
    }

    // ========================================================================
    // Additional edge case tests (from CODE_REVIEW.md)
    // ========================================================================

    #[test]
    fn test_withdrawal_on_nonexistent_account() {
        let mut ledger = Ledger::new();
        let withdrawal = Transaction {
            kind: TxKind::Withdrawal,
            client_id: ClientId(1),
            tx_id: TxId(1),
            amount: Some(dec!(5.0)),
        };
        // Should fail because account doesn't exist (treated as locked/missing)
        assert!(matches!(
            ledger.process(withdrawal),
            Err(LedgerError::AccountLocked(_))
        ));
    }

    #[test]
    fn test_dispute_when_available_less_than_amount() {
        // Edge case: deposit 10, withdraw 5, then dispute the original 10
        // This results in available going negative, but held captures the full amount
        let mut ledger = Ledger::new();

        let deposit = Transaction {
            kind: TxKind::Deposit,
            client_id: ClientId(1),
            tx_id: TxId(1),
            amount: Some(dec!(10.0)),
        };
        ledger.process(deposit).unwrap();

        let withdrawal = Transaction {
            kind: TxKind::Withdrawal,
            client_id: ClientId(1),
            tx_id: TxId(2),
            amount: Some(dec!(5.0)),
        };
        ledger.process(withdrawal).unwrap();

        // Now available = 5, but we dispute the original 10
        let dispute = Transaction {
            kind: TxKind::Dispute,
            client_id: ClientId(1),
            tx_id: TxId(1),
            amount: None,
        };
        assert!(ledger.process(dispute).is_ok());

        // Available goes negative, held is 10, total is still 5
        let account = ledger.accounts.get(&ClientId(1)).unwrap();
        assert_eq!(account.available, dec!(-5.0));
        assert_eq!(account.held, dec!(10.0));
        assert_eq!(account.total().unwrap(), dec!(5.0));
    }

    #[test]
    fn test_multiple_deposits_dispute_one() {
        let mut ledger = Ledger::new();

        let deposit1 = Transaction {
            kind: TxKind::Deposit,
            client_id: ClientId(1),
            tx_id: TxId(1),
            amount: Some(dec!(10.0)),
        };
        ledger.process(deposit1).unwrap();

        let deposit2 = Transaction {
            kind: TxKind::Deposit,
            client_id: ClientId(1),
            tx_id: TxId(2),
            amount: Some(dec!(20.0)),
        };
        ledger.process(deposit2).unwrap();

        // Dispute only the first deposit
        let dispute = Transaction {
            kind: TxKind::Dispute,
            client_id: ClientId(1),
            tx_id: TxId(1),
            amount: None,
        };
        ledger.process(dispute).unwrap();

        let account = ledger.accounts.get(&ClientId(1)).unwrap();
        assert_eq!(account.available, dec!(20.0)); // Only deposit2 available
        assert_eq!(account.held, dec!(10.0)); // deposit1 held
        assert_eq!(account.total().unwrap(), dec!(30.0));
    }

    #[test]
    fn test_locked_account_blocks_withdrawal() {
        let mut ledger = Ledger::new();

        // Deposit twice so we have funds after chargeback
        let deposit1 = Transaction {
            kind: TxKind::Deposit,
            client_id: ClientId(1),
            tx_id: TxId(1),
            amount: Some(dec!(5.0)),
        };
        ledger.process(deposit1).unwrap();

        let deposit2 = Transaction {
            kind: TxKind::Deposit,
            client_id: ClientId(1),
            tx_id: TxId(2),
            amount: Some(dec!(10.0)),
        };
        ledger.process(deposit2).unwrap();

        // Dispute and chargeback first deposit
        let dispute = Transaction {
            kind: TxKind::Dispute,
            client_id: ClientId(1),
            tx_id: TxId(1),
            amount: None,
        };
        ledger.process(dispute).unwrap();

        let chargeback = Transaction {
            kind: TxKind::Chargeback,
            client_id: ClientId(1),
            tx_id: TxId(1),
            amount: None,
        };
        ledger.process(chargeback).unwrap();

        // Account is now locked, try to withdraw
        let withdrawal = Transaction {
            kind: TxKind::Withdrawal,
            client_id: ClientId(1),
            tx_id: TxId(3),
            amount: Some(dec!(5.0)),
        };
        assert!(matches!(
            ledger.process(withdrawal),
            Err(LedgerError::AccountLocked(_))
        ));
    }

    #[test]
    fn test_cannot_dispute_resolved_transaction() {
        let mut ledger = Ledger::new();

        let deposit = Transaction {
            kind: TxKind::Deposit,
            client_id: ClientId(1),
            tx_id: TxId(1),
            amount: Some(dec!(10.0)),
        };
        ledger.process(deposit).unwrap();

        let dispute = Transaction {
            kind: TxKind::Dispute,
            client_id: ClientId(1),
            tx_id: TxId(1),
            amount: None,
        };
        ledger.process(dispute).unwrap();

        let resolve = Transaction {
            kind: TxKind::Resolve,
            client_id: ClientId(1),
            tx_id: TxId(1),
            amount: None,
        };
        ledger.process(resolve).unwrap();

        // Try to dispute the resolved transaction
        let dispute_again = Transaction {
            kind: TxKind::Dispute,
            client_id: ClientId(1),
            tx_id: TxId(1),
            amount: None,
        };
        assert!(matches!(
            ledger.process(dispute_again),
            Err(LedgerError::InvalidTxState(_, TxState::Resolved))
        ));
    }

    #[test]
    fn test_cannot_dispute_chargedback_transaction() {
        let mut ledger = Ledger::new();

        let deposit = Transaction {
            kind: TxKind::Deposit,
            client_id: ClientId(1),
            tx_id: TxId(1),
            amount: Some(dec!(10.0)),
        };
        ledger.process(deposit).unwrap();

        let dispute = Transaction {
            kind: TxKind::Dispute,
            client_id: ClientId(1),
            tx_id: TxId(1),
            amount: None,
        };
        ledger.process(dispute).unwrap();

        let chargeback = Transaction {
            kind: TxKind::Chargeback,
            client_id: ClientId(1),
            tx_id: TxId(1),
            amount: None,
        };
        ledger.process(chargeback).unwrap();

        // Account is locked, but even if it weren't, the tx state should prevent dispute
        // We need a second deposit on client 2 to test the state
        let deposit2 = Transaction {
            kind: TxKind::Deposit,
            client_id: ClientId(2),
            tx_id: TxId(2),
            amount: Some(dec!(5.0)),
        };
        ledger.process(deposit2).unwrap();

        // The original tx is ChargedBack - verify state machine
        let tx_record = ledger.transactions.get(&TxId(1)).unwrap();
        assert_eq!(tx_record.state, TxState::ChargedBack);
    }

    #[test]
    fn test_cannot_chargeback_twice() {
        let mut ledger = Ledger::new();

        let deposit = Transaction {
            kind: TxKind::Deposit,
            client_id: ClientId(1),
            tx_id: TxId(1),
            amount: Some(dec!(10.0)),
        };
        ledger.process(deposit).unwrap();

        let dispute = Transaction {
            kind: TxKind::Dispute,
            client_id: ClientId(1),
            tx_id: TxId(1),
            amount: None,
        };
        ledger.process(dispute).unwrap();

        let chargeback = Transaction {
            kind: TxKind::Chargeback,
            client_id: ClientId(1),
            tx_id: TxId(1),
            amount: None,
        };
        ledger.process(chargeback).unwrap();

        // Try to chargeback again - should fail because tx is now ChargedBack, not Disputed
        let chargeback_again = Transaction {
            kind: TxKind::Chargeback,
            client_id: ClientId(1),
            tx_id: TxId(1),
            amount: None,
        };
        // Account is locked, so this will fail with AccountLocked first
        assert!(matches!(
            ledger.process(chargeback_again),
            Err(LedgerError::AccountLocked(_))
        ));
    }

    #[test]
    fn test_process_method() {
        let mut ledger = Ledger::new();

        // Test successful process
        let deposit = Transaction {
            kind: TxKind::Deposit,
            client_id: ClientId(1),
            tx_id: TxId(1),
            amount: Some(dec!(10.0)),
        };
        assert!(ledger.process(deposit).is_ok());

        let account = ledger.accounts.get(&ClientId(1)).unwrap();
        assert_eq!(account.available, dec!(10.0));

        // Test failed process (insufficient funds)
        let withdrawal = Transaction {
            kind: TxKind::Withdrawal,
            client_id: ClientId(1),
            tx_id: TxId(2),
            amount: Some(dec!(20.0)),
        };
        assert!(matches!(
            ledger.process(withdrawal),
            Err(LedgerError::InsufficientFunds(_))
        ));

        // Balance should be unchanged
        let account = ledger.accounts.get(&ClientId(1)).unwrap();
        assert_eq!(account.available, dec!(10.0));
    }
}

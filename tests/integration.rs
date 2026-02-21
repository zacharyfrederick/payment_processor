use std::process::Command;

fn run_processor(fixture: &str) -> (String, String, bool) {
    let output = Command::new("cargo")
        .args(["run", "--", &format!("tests/fixtures/{}", fixture)])
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .output()
        .expect("failed to execute process");

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    (stdout, stderr, output.status.success())
}

fn parse_output(stdout: &str) -> Vec<(u16, String, String, String, bool)> {
    stdout
        .lines()
        .skip(1) // skip header
        .filter(|line| !line.is_empty())
        .map(|line| {
            let parts: Vec<&str> = line.split(',').collect();
            (
                parts[0].parse().unwrap(),
                parts[1].to_string(),
                parts[2].to_string(),
                parts[3].to_string(),
                parts[4].parse().unwrap(),
            )
        })
        .collect()
}

fn account_by_client(
    accounts: &[(u16, String, String, String, bool)],
    client: u16,
) -> Option<&(u16, String, String, String, bool)> {
    accounts.iter().find(|a| a.0 == client)
}

#[test]
fn test_basic_deposits_and_withdrawals() {
    let (stdout, _stderr, success) = run_processor("basic.csv");
    assert!(success);

    let accounts = parse_output(&stdout);
    assert_eq!(accounts.len(), 2);

    // Client 1: deposit 1.0 + deposit 2.0 - withdrawal 1.5 = 1.5
    let client1 = accounts.iter().find(|a| a.0 == 1).unwrap();
    assert_eq!(client1.1, "1.5000");
    assert_eq!(client1.2, "0.0000");
    assert_eq!(client1.3, "1.5000");
    assert!(!client1.4);

    // Client 2: deposit 2.0 - withdrawal 3.0 (rejected, insufficient funds) = 2.0
    let client2 = accounts.iter().find(|a| a.0 == 2).unwrap();
    assert_eq!(client2.1, "2.0000");
    assert_eq!(client2.2, "0.0000");
    assert_eq!(client2.3, "2.0000");
    assert!(!client2.4);
}

#[test]
fn test_dispute_resolve() {
    let (stdout, _stderr, success) = run_processor("dispute_resolve.csv");
    assert!(success);

    let accounts = parse_output(&stdout);
    assert_eq!(accounts.len(), 1);

    // Client 1: deposit 10.0, dispute, resolve -> back to 10.0 available
    let client1 = &accounts[0];
    assert_eq!(client1.0, 1);
    assert_eq!(client1.1, "10.0000");
    assert_eq!(client1.2, "0.0000");
    assert_eq!(client1.3, "10.0000");
    assert!(!client1.4);
}

#[test]
fn test_chargeback_locks_account() {
    let (stdout, stderr, success) = run_processor("chargeback.csv");
    assert!(success);

    let accounts = parse_output(&stdout);
    assert_eq!(accounts.len(), 1);

    // Client 1: deposit 10.0, dispute, chargeback -> 0.0 total, locked
    // Second deposit rejected because account is locked
    let client1 = &accounts[0];
    assert_eq!(client1.0, 1);
    assert_eq!(client1.1, "0.0000");
    assert_eq!(client1.2, "0.0000");
    assert_eq!(client1.3, "0.0000");
    assert!(client1.4); // locked

    // Should have a warning about the rejected deposit
    assert!(stderr.contains("locked"));
}

#[test]
fn test_whitespace_handling() {
    let (stdout, _stderr, success) = run_processor("whitespace.csv");
    assert!(success);

    let accounts = parse_output(&stdout);
    assert_eq!(accounts.len(), 1);

    // Client 1: deposit 1.5 - withdrawal 0.5 = 1.0
    let client1 = &accounts[0];
    assert_eq!(client1.1, "1.0000");
}

#[test]
fn test_decimal_precision() {
    let (stdout, _stderr, success) = run_processor("precision.csv");
    assert!(success);

    let accounts = parse_output(&stdout);
    assert_eq!(accounts.len(), 1);

    // Client 1: deposit 1.23456789 (rounds to 1.2346) + deposit 0.00005 (rounds to 0.0000)
    // Total = 1.2346
    let client1 = &accounts[0];
    assert_eq!(client1.1, "1.2346");
}

#[test]
fn test_comprehensive_fixture() {
    let (stdout, stderr, success) = run_processor("comprehensive.csv");
    assert!(success);

    let accounts = parse_output(&stdout);
    assert_eq!(accounts.len(), 4);

    // Client 1: basic + dispute/resolve + dispute(3) + chargeback(20) + locked; deposit 21 rejected
    let c1 = account_by_client(&accounts, 1).unwrap();
    assert_eq!(c1.1, "4.5000");
    assert_eq!(c1.2, "2.0000");
    assert_eq!(c1.3, "6.5000");
    assert!(c1.4);

    // Client 2: deposit 2, withdrawal 3 rejected (insufficient funds)
    let c2 = account_by_client(&accounts, 2).unwrap();
    assert_eq!(c2.1, "2.0000");
    assert_eq!(c2.2, "0.0000");
    assert_eq!(c2.3, "2.0000");
    assert!(!c2.4);

    // Client 3: deposits 30,31,32; resolve 31 and chargeback 32 rejected
    let c3 = account_by_client(&accounts, 3).unwrap();
    assert_eq!(c3.1, "120.0000");
    assert_eq!(c3.2, "0.0000");
    assert_eq!(c3.3, "120.0000");
    assert!(!c3.4);

    // Client 4: deposit 30 rejected (duplicate), dispute 30/999 rejected; deposits 40,41 (precision)
    let c4 = account_by_client(&accounts, 4).unwrap();
    assert_eq!(c4.1, "1.2346");
    assert_eq!(c4.2, "0.0000");
    assert_eq!(c4.3, "1.2346");
    assert!(!c4.4);

    // Verify rejected operations were logged. TxNotFound and TxClientMismatch not asserted:
    // dispute 4,30 and 4,999 run while client 4 has no account, so we hit AccountLocked first.
    assert!(stderr.contains("insufficient") || stderr.contains("InsufficientFunds"));
    assert!(stderr.contains("locked") || stderr.contains("AccountLocked"));
    assert!(stderr.contains("duplicate") || stderr.contains("DuplicateTxId"));
    assert!(stderr.contains("invalid state") || stderr.contains("InvalidTxState"));
}

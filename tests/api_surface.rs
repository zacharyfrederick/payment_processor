mod common;

use common::normalize_accounts_csv;
use std::process::Command;

#[test]
fn test_cli_interface() {
    let output = Command::new(env!("CARGO_BIN_EXE_payment-processor"))
        .arg("tests/fixtures/transactions.csv")
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .output()
        .expect("failed to execute binary");

    assert!(output.status.success());

    let stdout = String::from_utf8(output.stdout).unwrap();
    let expected = std::fs::read_to_string("tests/fixtures/expected.csv").unwrap();

    assert_eq!(
        normalize_accounts_csv(&stdout),
        normalize_accounts_csv(&expected)
    );
}

#[test]
fn test_cli_no_args_exits_with_error() {
    let output = Command::new(env!("CARGO_BIN_EXE_payment-processor"))
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .output()
        .expect("failed to execute binary");

    assert!(!output.status.success());
    assert!(output.stdout.is_empty());
    assert!(!output.stderr.is_empty());
}

#[test]
fn test_cli_missing_file_exits_with_error() {
    let output = Command::new(env!("CARGO_BIN_EXE_payment-processor"))
        .arg("nonexistent.csv")
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .output()
        .expect("failed to execute binary");

    assert!(!output.status.success());
    assert!(output.stdout.is_empty());
    assert!(!output.stderr.is_empty());
}

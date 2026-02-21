mod common;

use common::normalize_accounts_csv;

#[test]
fn test_full_pipeline() {
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_payment-processor"))
        .arg("tests/fixtures/golden.csv")
        .output()
        .expect("failed to run binary");

    let actual = String::from_utf8(output.stdout).unwrap();
    let expected = std::fs::read_to_string("tests/fixtures/expected.csv").unwrap();

    let actual_normalized = normalize_accounts_csv(&actual);
    let expected_normalized = normalize_accounts_csv(&expected);

    assert_eq!(actual_normalized, expected_normalized);
}

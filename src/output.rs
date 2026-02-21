use payments_core::{Account, ClientId};
use std::io::{self, Write};

/// Writes account data as CSV to the provided writer.
///
/// Output format:
/// ```csv
/// client,available,held,total,locked
/// 1,1.5000,0.0000,1.5000,false
/// ```
///
/// Amounts are formatted to exactly 4 decimal places.
pub fn write_accounts_csv<'a, W, I>(writer: &mut W, accounts: I) -> io::Result<()>
where
    W: Write,
    I: Iterator<Item = (&'a ClientId, &'a Account)>,
{
    writeln!(writer, "client,available,held,total,locked")?;

    for (client_id, account) in accounts {
        let Some(total) = account.total() else {
            eprintln!(
                "error: total overflow for client {} (available + held overflow)",
                client_id
            );
            continue;
        };
        writeln!(
            writer,
            "{},{:.4},{:.4},{:.4},{}",
            client_id, account.available, account.held, total, account.locked
        )?;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal_macros::dec;
    use std::collections::HashMap;

    #[test]
    fn test_write_single_account() {
        let mut accounts = HashMap::new();
        accounts.insert(
            ClientId(1),
            Account {
                available: dec!(1.5),
                held: dec!(0.0),
                locked: false,
            },
        );

        let mut output = Vec::new();
        write_accounts_csv(&mut output, accounts.iter()).unwrap();

        let result = String::from_utf8(output).unwrap();
        assert!(result.contains("client,available,held,total,locked"));
        assert!(result.contains("1,1.5000,0.0000,1.5000,false"));
    }

    #[test]
    fn test_write_multiple_accounts() {
        let mut accounts = HashMap::new();
        accounts.insert(
            ClientId(1),
            Account {
                available: dec!(1.5),
                held: dec!(0.0),
                locked: false,
            },
        );
        accounts.insert(
            ClientId(2),
            Account {
                available: dec!(2.0),
                held: dec!(1.0),
                locked: true,
            },
        );

        let mut output = Vec::new();
        write_accounts_csv(&mut output, accounts.iter()).unwrap();

        let result = String::from_utf8(output).unwrap();
        let lines: Vec<_> = result.lines().collect();
        assert_eq!(lines[0], "client,available,held,total,locked");
        assert_eq!(lines.len(), 3);
    }

    #[test]
    fn test_four_decimal_places() {
        let mut accounts = HashMap::new();
        accounts.insert(
            ClientId(1),
            Account {
                available: dec!(1),
                held: dec!(0),
                locked: false,
            },
        );

        let mut output = Vec::new();
        write_accounts_csv(&mut output, accounts.iter()).unwrap();

        let result = String::from_utf8(output).unwrap();
        assert!(result.contains("1.0000"));
    }

    #[test]
    fn test_total_derived() {
        let mut accounts = HashMap::new();
        accounts.insert(
            ClientId(1),
            Account {
                available: dec!(3.0),
                held: dec!(2.0),
                locked: false,
            },
        );

        let mut output = Vec::new();
        write_accounts_csv(&mut output, accounts.iter()).unwrap();

        let result = String::from_utf8(output).unwrap();
        assert!(result.contains("3.0000,2.0000,5.0000"));
    }

    #[test]
    fn test_empty_accounts() {
        let accounts: HashMap<ClientId, Account> = HashMap::new();

        let mut output = Vec::new();
        write_accounts_csv(&mut output, accounts.iter()).unwrap();

        let result = String::from_utf8(output).unwrap();
        assert_eq!(result, "client,available,held,total,locked\n");
    }
}

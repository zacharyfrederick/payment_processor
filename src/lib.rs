//! Library for the payment transaction processor.
//!
//! Exposes the sync runner, CLI args, and components for use by the default binary
//! and the async-bridge example.

pub mod csv_source;
pub mod error;
pub mod output;
pub mod processor;

pub use csv_source::CsvTransactionSource;
pub use output::write_accounts_csv;
pub use payments_core::{Ledger, Transaction};
pub use processor::Processor;

use clap::Parser;
use std::fs::File;
use std::io::{self, BufReader, BufWriter};
use std::path::PathBuf;

/// CLI arguments for the payment processor.
///
/// Reads transactions from a CSV file and outputs the final account states.
#[derive(Parser, Debug)]
#[command(name = "payment-processor")]
#[command(version, about)]
pub struct Args {
    /// Path to the transactions CSV file
    #[arg(value_name = "FILE")]
    pub input: PathBuf,
}

/// Runs the payment processor synchronously: read CSV from file, process, write CSV to stdout.
pub fn run_sync(args: &Args) -> io::Result<()> {
    let file = File::open(&args.input)?;
    let source = CsvTransactionSource::new(BufReader::new(file));
    let ledger = Processor::new(source).run();

    let stdout = io::stdout();
    let mut writer = BufWriter::new(stdout.lock());
    write_accounts_csv(&mut writer, ledger.iter_accounts())?;

    Ok(())
}

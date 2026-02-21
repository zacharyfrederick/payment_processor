//! Default binary: sync payment processor.

use clap::Parser;
use payment_processor::{run_sync, Args};
use std::process;

fn main() {
    let args = Args::parse();
    if let Err(e) = run_sync(&args) {
        eprintln!("error: {e}");
        process::exit(1);
    }
}

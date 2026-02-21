//! Async bridge example: many concurrent streams feeding a single ledger.
//!
//! Simulates the "thousands of concurrent TCP streams" scenario: multiple input
//! streams (here, CSV files) are read concurrently; transactions are sent to a
//! single processor task that owns the Ledger. When all streams have finished
//! (all senders dropped), the processor outputs the final accounts CSV once.
//!
//! **Requires the `async` feature.** Run with:
//!   `cargo run --example async_bridge --features async -- file1.csv file2.csv > accounts.csv`
#[cfg(feature = "async")]
use clap::Parser;


#[cfg(not(feature = "async"))]
fn main() {
    eprintln!("Build with --features async to enable this example.");
    eprintln!("Example: cargo run --example async_bridge --features async -- file1.csv file2.csv > accounts.csv");
    std::process::exit(1);
}

#[cfg(feature = "async")]
#[tokio::main]
async fn main() {
    let args = async_bridge::Args::parse();
    if args.inputs.is_empty() {
        eprintln!("error: at least one input file required");
        std::process::exit(1);
    }
    if let Err(e) = async_bridge::run_concurrent_streams(&args).await {
        eprintln!("error: {e}");
        std::process::exit(1);
    }
}

#[cfg(feature = "async")]
mod async_bridge {
    use clap::Parser;
    use payment_processor::{write_accounts_csv, CsvTransactionSource, Ledger, Transaction};
    use std::io::Cursor;
    use std::path::PathBuf;
    use tokio::io::AsyncWriteExt;
    use tokio::sync::mpsc;

    /// Multiple input files = multiple concurrent streams.
    #[derive(Parser, Debug)]
    #[command(name = "async_bridge")]
    #[command(about = "Process multiple CSV streams concurrently into one ledger")]
    pub struct Args {
        /// Paths to transaction CSV files (each is one stream)
        #[arg(value_name = "FILE", num_args = 1..)]
        pub inputs: Vec<PathBuf>,
    }

    pub async fn run_concurrent_streams(args: &Args) -> std::io::Result<()> {
        let (tx_send, mut tx_recv) = mpsc::unbounded_channel::<Transaction>();

        let mut send_handles = Vec::with_capacity(args.inputs.len());
        for (stream_id, path) in args.inputs.iter().enumerate() {
            let path = path.clone();
            let sender = tx_send.clone();
            let handle = tokio::spawn(async move {
                stream_producer(stream_id, path, sender).await;
            });
            send_handles.push(handle);
        }

        drop(tx_send);

        let processor = tokio::spawn(async move {
            let mut ledger = Ledger::new();
            while let Some(transaction) = tx_recv.recv().await {
                if let Err(e) = ledger.process(transaction) {
                    eprintln!("warn: {e}");
                }
            }
            ledger
        });

        for h in send_handles {
            let _ = h.await;
        }

        let ledger = processor
            .await
            .map_err(|e| std::io::Error::other(format!("processor join: {e}")))?;

        let mut buffer = Vec::new();
        write_accounts_csv(&mut buffer, ledger.iter_accounts())?;
        let mut stdout = tokio::io::stdout();
        stdout.write_all(&buffer).await?;

        Ok(())
    }

    async fn stream_producer(
        stream_id: usize,
        path: PathBuf,
        sender: mpsc::UnboundedSender<Transaction>,
    ) {
        let contents = match tokio::fs::read_to_string(&path).await {
            Ok(c) => c,
            Err(e) => {
                eprintln!(
                    "warn: stream {} ({}): read failed: {e}",
                    stream_id,
                    path.display()
                );
                return;
            }
        };

        let source = CsvTransactionSource::new(Cursor::new(contents));
        for (row, result) in source.enumerate() {
            match result {
                Ok(tx) => {
                    if sender.send(tx).is_err() {
                        break;
                    }
                }
                Err(e) => {
                    eprintln!("warn: stream {} row {}: {e}", stream_id, row + 2);
                }
            }
        }
    }
}

use clap::Parser;
use sim_relay::source;
use std::sync::mpsc;

fn main() -> std::io::Result<()> {
    let args = source::Args::parse();

    let (tx, rx) = mpsc::channel::<()>();
    ctrlc::set_handler(move || {
        println!("\nShutting down...");
        let _ = tx.send(());
    })
    .expect("failed to install Ctrl-C handler");

    source::run(args, rx)
}

use clap::Parser;
use sim_relay::target;
use std::sync::mpsc;

fn main() -> std::io::Result<()> {
    let args = target::Args::parse();

    let (tx, rx) = mpsc::channel::<()>();
    ctrlc::set_handler(move || {
        println!("\nShutting down...");
        let _ = tx.send(());
    })
    .expect("failed to install Ctrl-C handler");

    target::run(args, rx)
}

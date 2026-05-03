mod config;
mod install;
mod logger;
mod scanner;
mod source;
mod target;

use clap::{Parser, Subcommand};
use std::sync::mpsc;

const VERSION: &str = env!("CARGO_PKG_VERSION");

#[derive(Parser)]
#[command(name = "sim-bridge", version, about = "Unified launcher for iRacing, AC, and Sim Relay telemetry")]
struct Cli {
    #[command(subcommand)]
    command: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Gaming PC — auto-detects running games and starts the correct telemetry app.
    Source,
    /// SimHub PC — starts all three telemetry receivers simultaneously.
    Target,
    /// Interactive first-run setup wizard. Writes sim-bridge.toml.
    Setup,
    /// Add sim-bridge to Windows startup (Task Scheduler, runs on logon).
    Install,
    /// Remove sim-bridge from Windows startup.
    Uninstall,
}

fn main() {
    let cli = Cli::parse();
    match cli.command {
        Cmd::Source => run_source(),
        Cmd::Target => run_target(),
        Cmd::Setup => run_setup(),
        Cmd::Install => run_install(),
        Cmd::Uninstall => run_uninstall(),
    }
}

fn run_source() {
    let log = logger::Logger::open().unwrap_or_else(|e| {
        eprintln!("Warning: could not open log file: {e}");
        logger::Logger::stderr()
    });
    log.log(&format!("sim-bridge v{VERSION} — source"));

    let cfg = match config::load() {
        Ok(c) => c,
        Err(e) => {
            eprintln!("Config error: {e}");
            std::process::exit(1);
        }
    };

    let (tx, rx) = mpsc::channel::<()>();
    ctrlc::set_handler(move || {
        println!("\nCtrl-C received...");
        let _ = tx.send(());
    })
    .expect("failed to install Ctrl-C handler");

    source::run(cfg, &log, rx);
}

fn run_target() {
    let log = logger::Logger::open().unwrap_or_else(|e| {
        eprintln!("Warning: could not open log file: {e}");
        logger::Logger::stderr()
    });
    log.log(&format!("sim-bridge v{VERSION} — target"));

    let cfg = match config::load() {
        Ok(c) => c,
        Err(e) => {
            eprintln!("Config error: {e}");
            std::process::exit(1);
        }
    };

    let (tx, rx) = mpsc::channel::<()>();
    ctrlc::set_handler(move || {
        println!("\nCtrl-C received...");
        let _ = tx.send(());
    })
    .expect("failed to install Ctrl-C handler");

    target::run(cfg, &log, rx);
}

fn run_setup() {
    match config::setup_wizard() {
        Ok(_) => {}
        Err(e) => {
            eprintln!("Setup failed: {e}");
            std::process::exit(1);
        }
    }
}

fn run_install() {
    if let Err(e) = install::install("source") {
        eprintln!("Install failed: {e}");
        std::process::exit(1);
    }
}

fn run_uninstall() {
    if let Err(e) = install::uninstall() {
        eprintln!("Uninstall failed: {e}");
        std::process::exit(1);
    }
}

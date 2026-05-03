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
    Install {
        /// Which mode to register: source (gaming PC) or target (SimHub PC).
        /// Defaults to the mode stored in sim-bridge.toml, or "source" if no config.
        #[arg(long, value_name = "MODE")]
        mode: Option<String>,
    },
    /// Remove sim-bridge from Windows startup.
    Uninstall,
    /// List all supported games (iRacing, AC family, and all sim-relay UDP games).
    List,
}

fn main() {
    let cli = Cli::parse();
    match cli.command {
        Cmd::Source => run_source(),
        Cmd::Target => run_target(),
        Cmd::Setup => run_setup(),
        Cmd::Install { mode } => run_install(mode),
        Cmd::Uninstall => run_uninstall(),
        Cmd::List => run_list(),
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

fn run_install(mode_arg: Option<String>) {
    let mode = mode_arg.unwrap_or_else(|| {
        config::load()
            .map(|c| c.mode)
            .unwrap_or_else(|_| "source".to_string())
    });

    if mode != "source" && mode != "target" {
        eprintln!("Invalid mode '{}'. Use 'source' or 'target'.", mode);
        std::process::exit(1);
    }

    if let Err(e) = install::install(&mode) {
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

fn run_list() {
    println!("sim-bridge — Supported Games");
    println!();
    println!("Shared Memory (process-detected, auto-started by sim-bridge source):");
    println!("  {:<35} {:<35} Port", "Game", "Process");
    println!("  {}", "-".repeat(80));
    println!("  {:<35} {:<35} 5000", "iRacing", "iRacingSim64DX11.exe");
    println!("  {:<35} {:<35} 5001", "Assetto Corsa EVO", "AssettoCorsa_EVO.exe");
    println!("  {:<35} {:<35} 5001", "Assetto Corsa", "acs.exe");
    println!("  {:<35} {:<35} 5001", "Assetto Corsa Competizione", "acc.exe");
    println!();
    println!("UDP Relay (auto-detected by sim-relay, always running on source):");
    for game in sim_relay::games::GAMES {
        let port = game.default_port;
        println!("  {:<35} port {port}", game.name);
    }
}

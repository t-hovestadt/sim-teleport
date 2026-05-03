use sim_relay::{games, source, target};

use std::io;
use std::sync::mpsc;

use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(
    name = "sim-relay",
    version,
    about = "Forward game UDP telemetry to a remote SimHub PC"
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Capture game telemetry on this PC and forward to a target PC
    Source {
        /// Target PC IP address
        #[arg(long, value_name = "IP")]
        target: String,
        /// Comma-separated game IDs to forward (default: auto-detect all)
        #[arg(long, value_name = "ID,...", value_delimiter = ',')]
        games: Option<Vec<String>>,
        /// Bind all ports immediately, skip process detection
        #[arg(long)]
        all: bool,
        /// Also forward to localhost:<port+1000> for a local SimHub instance
        #[arg(long)]
        local_forward: bool,
        /// Bind address for listening sockets
        #[arg(long, default_value = "0.0.0.0")]
        bind: String,
        /// Set HIGH_PRIORITY_CLASS for this process
        #[arg(long)]
        high_priority: bool,
        /// How often to scan for game processes (seconds)
        #[arg(long, default_value = "5", value_name = "SECS")]
        scan_interval: u64,
        /// How long to keep forwarding after a game exits (seconds)
        #[arg(long, default_value = "15", value_name = "SECS")]
        grace_period: u64,
        /// Include console-only games (GT7, GT Sport) in auto-detect mode
        #[arg(long)]
        include_console: bool,
        /// Bind all ports immediately, skip process detection (alias for --all)
        #[arg(long)]
        force_bind: bool,
    },
    /// Receive forwarded telemetry and relay to SimHub on this PC
    Target {
        /// Source PC IP address (informational only)
        #[arg(long, value_name = "IP")]
        source: Option<String>,
        /// Comma-separated game IDs to listen for (default: all)
        #[arg(long, value_name = "ID,...", value_delimiter = ',')]
        games: Option<Vec<String>>,
        /// Listen on all supported game ports
        #[arg(long)]
        all: bool,
        /// Override where to forward received packets (default: 127.0.0.1:<game_port>)
        #[arg(long, value_name = "IP:PORT")]
        forward_to: Option<String>,
        /// Set HIGH_PRIORITY_CLASS for this process
        #[arg(long)]
        high_priority: bool,
        /// Spin on recv instead of sleeping (lower latency, higher CPU)
        #[arg(long)]
        busy_wait: bool,
    },
    /// List supported games, ports, and setup instructions
    List,
}

fn main() -> io::Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Command::Source {
            target,
            games,
            all,
            local_forward,
            bind,
            high_priority,
            scan_interval,
            grace_period,
            include_console,
            force_bind,
        } => {
            let (tx, rx) = mpsc::channel::<()>();
            ctrlc::set_handler(move || {
                println!("\nShutting down...");
                let _ = tx.send(());
            })
            .expect("failed to install Ctrl-C handler");
            source::run(
                source::Args {
                    target,
                    games,
                    all,
                    local_forward,
                    bind,
                    high_priority,
                    scan_interval,
                    grace_period,
                    include_console,
                    force_bind,
                },
                rx,
            )
        }
        Command::Target {
            source,
            games,
            all,
            forward_to,
            high_priority,
            busy_wait,
        } => {
            let (tx, rx) = mpsc::channel::<()>();
            ctrlc::set_handler(move || {
                println!("\nShutting down...");
                let _ = tx.send(());
            })
            .expect("failed to install Ctrl-C handler");
            target::run(
                target::Args {
                    source,
                    games,
                    all,
                    forward_to,
                    high_priority,
                    busy_wait,
                    on_game_active: None,
                },
                rx,
            )
        }
        Command::List => {
            games::print_list();
            Ok(())
        }
    }
}

use ac_teleport::{game, source, target};
use clap::{Parser, Subcommand};
use std::sync::mpsc;
use std::time::Duration;

const DEFAULT_MULTICAST: &str = "239.255.0.1";
const DEFAULT_PORT: u16 = 5001;

/// Stream Assetto Corsa (AC1) or Assetto Corsa EVO telemetry over the network
/// so SimHub can run on a different machine than your game PC.
///
/// Both source and target auto-detect which game is running. The --game flag
/// is only needed when you want to force a specific game.
#[derive(Parser)]
#[command(name = "ac-teleport", version, about)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Read game telemetry from shared memory and broadcast it over UDP.
    ///
    /// Automatically detects Assetto Corsa EVO or AC1. Use --game to force one.
    Source {
        /// Game override: "ac1" or "evo". Default: auto-detect (EVO → AC1 priority).
        #[arg(long)]
        game: Option<String>,

        /// Destination — multicast group:port, or unicast target address.
        #[arg(long, default_value_t = format!("{DEFAULT_MULTICAST}:{DEFAULT_PORT}"))]
        target: String,

        /// Local address to bind the UDP socket to.
        #[arg(long, default_value = "0.0.0.0:0")]
        bind: String,

        /// Send directly to one host instead of multicast.
        #[arg(long)]
        unicast: bool,

        /// Spin between polls instead of sleeping (lower latency, higher CPU).
        #[arg(long)]
        busy_wait: bool,

        /// Pin the polling thread to a specific CPU core (0-based index).
        #[arg(long)]
        pin_core: Option<usize>,

        /// Raise to HIGH_PRIORITY_CLASS (process) + ABOVE_NORMAL (thread). Safe on the
        /// SimHub PC; on the game PC only use if the game is not running on the same machine.
        #[arg(long)]
        high_priority: bool,

        /// Polling rate in Hz (default: 60).
        #[arg(long, default_value_t = 60)]
        poll_rate: u32,
    },

    /// Receive telemetry and expose it as local shared memory for SimHub.
    ///
    /// Without --game, creates maps for both EVO and AC1 simultaneously so
    /// SimHub finds whichever game is active.
    Target {
        /// Game override: "ac1" or "evo". Default: create maps for both games.
        #[arg(long)]
        game: Option<String>,

        /// Address and port to listen on.
        #[arg(long, default_value_t = format!("0.0.0.0:{DEFAULT_PORT}"))]
        bind: String,

        /// Multicast group to join (ignored in unicast mode).
        #[arg(long, default_value = DEFAULT_MULTICAST)]
        group: String,

        /// Expect a direct unicast stream instead of multicast.
        #[arg(long)]
        unicast: bool,

        /// Spin on recv instead of blocking (lower latency, higher CPU).
        #[arg(long)]
        busy_wait: bool,

        /// Pin the receive thread to a specific CPU core (0-based index).
        #[arg(long)]
        pin_core: Option<usize>,

        /// Raise to HIGH_PRIORITY_CLASS (process) + ABOVE_NORMAL (thread). Safe on the
        /// SimHub PC; on the game PC only use if the game is not running on the same machine.
        #[arg(long)]
        high_priority: bool,

        /// Seconds without data before action: drop maps (single-game mode) or zero game status (dual mode).
        #[arg(long, default_value_t = 10)]
        stale_timeout: u64,
    },
}

fn main() {
    let cli = Cli::parse();

    let (tx, rx) = mpsc::channel::<()>();
    ctrlc::set_handler(move || {
        println!("\nShutting down...");
        let _ = tx.send(());
    })
    .expect("failed to install Ctrl-C handler");

    let result = match cli.command {
        Command::Source {
            game: game_id,
            target,
            bind,
            unicast,
            busy_wait,
            pin_core,
            high_priority,
            poll_rate,
        } => {
            let game_cfg = game_id.as_deref().map(resolve_game);
            let mode = if unicast { "unicast" } else { "multicast" };
            match game_cfg {
                Some(g) => println!("{} source → {target} ({mode})", g.name),
                None => println!("AC Teleport source → {target} ({mode}) [auto-detect]"),
            }
            source::run(
                source::SourceArgs {
                    game: game_cfg,
                    target,
                    bind,
                    unicast,
                    busy_wait,
                    pin_core,
                    high_priority,
                    poll_rate,
                },
                rx,
            )
        }

        Command::Target {
            game: game_id,
            bind,
            group,
            unicast,
            busy_wait,
            pin_core,
            high_priority,
            stale_timeout,
        } => {
            let game_cfg = game_id.as_deref().map(resolve_game);
            let dest = if unicast { "unicast" } else { group.as_str() };
            let mode = if unicast { "unicast" } else { "multicast" };
            match game_cfg {
                Some(g) => println!("{} target ← {dest} ({mode})", g.name),
                None => println!("AC Teleport target ← {dest} ({mode}) [dual-map: EVO + AC1]"),
            }
            target::run(
                target::TargetArgs {
                    game: game_cfg,
                    bind,
                    group,
                    unicast,
                    busy_wait,
                    pin_core,
                    high_priority,
                    stale_timeout: Duration::from_secs(stale_timeout),
                },
                rx,
            )
        }
    };

    if let Err(e) = result {
        eprintln!("error: {e}");
        std::process::exit(1);
    }
}

fn resolve_game(id: &str) -> &'static game::GameConfig {
    match game::resolve(id) {
        Some(g) => g,
        None => {
            eprintln!("Unknown game '{id}'. Valid options: ac1, evo");
            std::process::exit(1);
        }
    }
}

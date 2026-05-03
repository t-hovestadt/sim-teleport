use ac_teleport::{game, target};
use clap::Parser;
use std::sync::mpsc;
use std::time::Duration;

/// Receive AC1 or ACE telemetry and expose it as local shared memory for SimHub.
///
/// Same as `ac-teleport target` but without the subcommand — pass options
/// directly: `target.exe`
///
/// Without --game, creates maps for both EVO and AC1 simultaneously so SimHub
/// finds whichever game is active.
#[derive(Parser)]
#[command(name = "target", version, about)]
struct Args {
    /// Game override: "ac1" or "evo". Default: create maps for both games.
    #[arg(long)]
    game: Option<String>,

    /// Address and port to listen on.
    #[arg(long, default_value = "0.0.0.0:5001")]
    bind: String,

    /// Multicast group to join (ignored in unicast mode).
    #[arg(long, default_value = "239.255.0.1")]
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

    /// Raise to HIGH_PRIORITY_CLASS (process) + ABOVE_NORMAL (thread). Safe on
    /// the SimHub PC; on the game PC only use if the game is not running on the same machine.
    #[arg(long)]
    high_priority: bool,

    /// Seconds without data before action: drop maps (single-game mode) or zero game status (dual mode).
    #[arg(long, default_value_t = 10)]
    stale_timeout: u64,
}

fn main() {
    let args = Args::parse();

    let game_cfg = match args.game.as_deref() {
        Some(id) => match game::resolve(id) {
            Some(g) => Some(g),
            None => {
                eprintln!("Unknown game '{id}'. Valid options: ac1, evo");
                std::process::exit(1);
            }
        },
        None => None,
    };

    let (tx, rx) = mpsc::channel::<()>();
    ctrlc::set_handler(move || {
        println!("\nShutting down...");
        let _ = tx.send(());
    })
    .expect("failed to install Ctrl-C handler");

    let dest = if args.unicast {
        "unicast"
    } else {
        args.group.as_str()
    };
    let mode = if args.unicast { "unicast" } else { "multicast" };
    match game_cfg {
        Some(g) => println!("{} target ← {dest} ({mode})", g.name),
        None => println!("AC Teleport target ← {dest} ({mode}) [dual-map: EVO + AC1]"),
    }

    if let Err(e) = target::run(
        target::TargetArgs {
            game: game_cfg,
            bind: args.bind,
            group: args.group,
            unicast: args.unicast,
            busy_wait: args.busy_wait,
            pin_core: args.pin_core,
            high_priority: args.high_priority,
            stale_timeout: Duration::from_secs(args.stale_timeout),
            on_first_data: None,
            on_stale: None,
        },
        rx,
    ) {
        eprintln!("error: {e}");
        std::process::exit(1);
    }
}

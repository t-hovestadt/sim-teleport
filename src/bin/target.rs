use ac_teleport::{game, target};
use clap::Parser;
use std::sync::mpsc;
use std::time::Duration;

/// Receive AC1 or ACE telemetry and expose it as local shared memory for SimHub.
///
/// Same as `ac-teleport target` but without the subcommand — pass --game and
/// any options directly: `target.exe --game ac1`
#[derive(Parser)]
#[command(name = "target", version, about)]
struct Args {
    /// Game to mirror: "ac1" for Assetto Corsa, "evo" for Assetto Corsa EVO.
    #[arg(long)]
    game: String,

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

    /// Drop shared memory maps after this many seconds without data.
    #[arg(long, default_value_t = 10)]
    stale_timeout: u64,
}

fn main() {
    let args = Args::parse();

    let game_cfg = match game::resolve(&args.game) {
        Some(g) => g,
        None => {
            eprintln!("Unknown game '{}'. Valid options: ac1, evo", args.game);
            std::process::exit(1);
        }
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
    println!("{} target ← {dest} ({mode})", game_cfg.name);

    if let Err(e) = target::run(
        game_cfg,
        target::TargetArgs {
            bind: args.bind,
            group: args.group,
            unicast: args.unicast,
            busy_wait: args.busy_wait,
            pin_core: args.pin_core,
            high_priority: args.high_priority,
            stale_timeout: Duration::from_secs(args.stale_timeout),
        },
        rx,
    ) {
        eprintln!("error: {e}");
        std::process::exit(1);
    }
}

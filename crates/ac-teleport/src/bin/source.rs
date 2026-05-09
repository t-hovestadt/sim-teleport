use ac_teleport::{game, source};
use clap::Parser;
use std::sync::mpsc;

/// Read AC1 or ACE telemetry from shared memory and broadcast it over UDP.
///
/// Same as `ac-teleport source` but without the subcommand — pass options
/// directly: `source.exe --target 192.168.50.2:5001`
///
/// Auto-detects which game is running (EVO → AC1 priority). Use --game to force one.
#[derive(Parser)]
#[command(name = "source", version, about)]
struct Args {
    /// Game override: "ac1" or "evo". Default: auto-detect (EVO → AC1 priority).
    #[arg(long)]
    game: Option<String>,

    /// Destination — multicast group:port, or unicast target address.
    #[arg(long, default_value = "239.255.0.1:5001")]
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

    /// Raise to HIGH_PRIORITY_CLASS (process) + ABOVE_NORMAL (thread). Safe on
    /// the SimHub PC; on the game PC only use if the game is not running on the same machine.
    #[arg(long)]
    high_priority: bool,

    /// Polling rate in Hz.
    #[arg(long, default_value_t = 60)]
    poll_rate: u32,
    /// Print a hex dump of the first 64 bytes of EVO physics/graphics maps
    /// once per second. Use this to locate packetId when the struct layout
    /// is unknown. No effect on AC1.
    #[arg(long)]
    verbose: bool,
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

    let mode = if args.unicast { "unicast" } else { "multicast" };
    match game_cfg {
        Some(g) => println!("{} source → {} ({mode})", g.name, args.target),
        None => println!(
            "AC Teleport source → {} ({mode}) [auto-detect]",
            args.target
        ),
    }

    if let Err(e) = source::run(
        source::SourceArgs {
            game: game_cfg,
            target: args.target,
            bind: args.bind,
            unicast: args.unicast,
            busy_wait: args.busy_wait,
            pin_core: args.pin_core,
            high_priority: args.high_priority,
            poll_rate: args.poll_rate,
            verbose: args.verbose,
        },
        rx,
    ) {
        eprintln!("error: {e}");
        std::process::exit(1);
    }
}

use ac_teleport::{game, source};
use clap::Parser;
use std::sync::mpsc;

/// Read AC1 or ACE telemetry from shared memory and broadcast it over UDP.
///
/// Same as `ac-teleport source` but without the subcommand — pass --game and
/// any options directly: `source.exe --game ac1`
#[derive(Parser)]
#[command(name = "source", version, about)]
struct Args {
    /// Game to relay: "ac1" for Assetto Corsa, "evo" for Assetto Corsa EVO.
    #[arg(long)]
    game: String,

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

    let mode = if args.unicast { "unicast" } else { "multicast" };
    println!("{} source → {} ({mode})", game_cfg.name, args.target);

    if let Err(e) = source::run(
        game_cfg,
        source::SourceArgs {
            target: args.target,
            bind: args.bind,
            unicast: args.unicast,
            busy_wait: args.busy_wait,
            pin_core: args.pin_core,
            high_priority: args.high_priority,
            poll_rate: args.poll_rate,
        },
        rx,
    ) {
        eprintln!("error: {e}");
        std::process::exit(1);
    }
}

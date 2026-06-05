mod config;
mod cpu;
mod install;
mod logger;
mod report;
mod scanner;
mod simhub_setup;
mod source;
mod steam;
mod stub;
mod target;

use clap::{Parser, Subcommand};
use std::sync::mpsc;

const VERSION: &str = env!("CARGO_PKG_VERSION");
const TELEPORT_VERSION: &str = env!("IRACING_TELEPORT_VERSION");
const AC_VERSION: &str = env!("AC_TELEPORT_VERSION");
const RELAY_VERSION: &str = env!("SIM_RELAY_VERSION");

#[derive(Parser)]
#[command(
    name = "sim-teleport",
    version = concat!(
        env!("CARGO_PKG_VERSION"),
        " (iracing-teleport ", env!("IRACING_TELEPORT_VERSION"),
        ", ac-teleport ", env!("AC_TELEPORT_VERSION"),
        ", sim-relay ", env!("SIM_RELAY_VERSION"), ")"
    ),
    about = "Unified launcher for iRacing, AC, and Sim Relay telemetry"
)]
struct Cli {
    #[command(subcommand)]
    command: Option<Cmd>,
}

#[derive(Subcommand)]
enum Cmd {
    /// Gaming PC — auto-detects running games and starts the correct telemetry app.
    Source {
        /// Target PC IP address. Required for Sim Relay forwarding; also used for unicast.
        #[arg(long)]
        target: Option<String>,
        /// This PC's bind IP address. Required for unicast mode.
        #[arg(long)]
        bind: Option<String>,
        /// Unicast mode — use for direct ethernet (point-to-point), not LAN.
        #[arg(long)]
        unicast: bool,
        /// Set HIGH_PRIORITY_CLASS on telemetry threads.
        #[arg(long)]
        high_priority: bool,
        /// Spin-wait instead of sleeping (lower jitter, burns one CPU core).
        #[arg(long)]
        busy_wait: bool,
        /// iRacing Teleport port.
        #[arg(long, value_name = "PORT")]
        iracing_port: Option<u16>,
        /// AC Teleport port.
        #[arg(long, value_name = "PORT")]
        ac_port: Option<u16>,
        /// Disable iRacing Teleport.
        #[arg(long)]
        no_iracing: bool,
        /// Disable AC Teleport.
        #[arg(long)]
        no_ac: bool,
        /// Disable Sim Relay.
        #[arg(long)]
        no_relay: bool,
        /// Process scan interval in seconds.
        #[arg(long, value_name = "SECS")]
        scan_interval: Option<u64>,
        /// Grace period after game closes before stopping.
        #[arg(long, value_name = "SECS")]
        drain: Option<u64>,
        /// Print detailed detection results each scan cycle (probe outcomes, tiebreakers, process matches).
        #[arg(long)]
        verbose: bool,
        /// Port offset for Sim Relay forwarding (default 10000). Source sends to target:(game_port+offset).
        #[arg(long, value_name = "N")]
        port_offset: Option<u16>,
        /// Skip CPU 0 exclusion (for non-iRacing sims or when using Process Lasso).
        #[arg(long)]
        no_cpu_exclude: bool,
    },
    /// SimHub PC — starts all three telemetry receivers simultaneously.
    Target {
        /// Source (gaming) PC IP address. Passed to Sim Relay for filtering.
        #[arg(long)]
        source: Option<String>,
        /// Unicast mode — use for direct ethernet (point-to-point), not LAN.
        #[arg(long)]
        unicast: bool,
        /// Set HIGH_PRIORITY_CLASS on telemetry threads.
        #[arg(long)]
        high_priority: bool,
        /// Spin-wait instead of sleeping (lower jitter, burns one CPU core).
        #[arg(long)]
        busy_wait: bool,
        /// iRacing Teleport port.
        #[arg(long, value_name = "PORT")]
        iracing_port: Option<u16>,
        /// AC Teleport port.
        #[arg(long, value_name = "PORT")]
        ac_port: Option<u16>,
        /// Disable iRacing Teleport.
        #[arg(long)]
        no_iracing: bool,
        /// Disable AC Teleport.
        #[arg(long)]
        no_ac: bool,
        /// Disable Sim Relay.
        #[arg(long)]
        no_relay: bool,
        /// Enable FanaLab shared-memory output.
        #[arg(long)]
        fanalab: bool,
        /// Write iRacing .ibt telemetry files on the target PC (for Garage 61 etc.).
        #[arg(long)]
        write_ibt: bool,
        /// Port offset for Sim Relay (default 10000). Target listens on (game_port+offset), forwards to game_port.
        #[arg(long, value_name = "N")]
        port_offset: Option<u16>,
    },
    /// Interactive setup wizard. Writes sim-teleport.toml (optional — CLI flags work without it).
    Setup,
    /// Add sim-teleport to Windows startup (Task Scheduler, runs on logon).
    Install {
        /// Which mode to register: source (gaming PC) or target (SimHub PC).
        /// Defaults to the mode stored in sim-teleport.toml, or "source" if no config.
        #[arg(long, value_name = "MODE")]
        mode: Option<String>,
    },
    /// Remove sim-teleport from Windows startup.
    Uninstall,
    /// List all supported games (iRacing, AC family, and all sim-relay UDP games).
    List {
        /// Include process names and ports for each game.
        #[arg(long)]
        verbose: bool,
    },
    /// Print Windows Firewall rules needed for all configured ports.
    Firewall,
    /// Internal: sleep forever (used as named stub process for SimHub game detection).
    #[command(hide = true)]
    Stub,
    /// Internal: write HKLM registry entries for AC install paths (requires admin).
    #[command(hide = true)]
    RegSetup,
}

fn main() {
    let cli = Cli::parse();
    let cmd = match cli.command {
        Some(c) => c,
        // No subcommand: auto-detect from saved config mode, default to source.
        None => {
            let mode = config::try_load()
                .map(|c| c.mode)
                .unwrap_or_else(|| "source".to_string());
            if mode == "target" {
                Cmd::Target {
                    source: None,
                    unicast: false,
                    high_priority: false,
                    busy_wait: false,
                    iracing_port: None,
                    ac_port: None,
                    no_iracing: false,
                    no_ac: false,
                    no_relay: false,
                    fanalab: false,
                    write_ibt: false,
                    port_offset: None,
                }
            } else {
                Cmd::Source {
                    target: None,
                    bind: None,
                    unicast: false,
                    high_priority: false,
                    busy_wait: false,
                    iracing_port: None,
                    ac_port: None,
                    no_iracing: false,
                    no_ac: false,
                    no_relay: false,
                    scan_interval: None,
                    drain: None,
                    verbose: false,
                    port_offset: None,
                    no_cpu_exclude: false,
                }
            }
        }
    };
    match cmd {
        Cmd::Source {
            target,
            bind,
            unicast,
            high_priority,
            busy_wait,
            iracing_port,
            ac_port,
            no_iracing,
            no_ac,
            no_relay,
            scan_interval,
            drain,
            verbose,
            port_offset,
            no_cpu_exclude,
        } => run_source(
            target,
            bind,
            unicast,
            high_priority,
            busy_wait,
            iracing_port,
            ac_port,
            no_iracing,
            no_ac,
            no_relay,
            scan_interval,
            drain,
            verbose,
            port_offset,
            no_cpu_exclude,
        ),
        Cmd::Target {
            source,
            unicast,
            high_priority,
            busy_wait,
            iracing_port,
            ac_port,
            no_iracing,
            no_ac,
            no_relay,
            fanalab,
            write_ibt,
            port_offset,
        } => run_target(
            source,
            unicast,
            high_priority,
            busy_wait,
            iracing_port,
            ac_port,
            no_iracing,
            no_ac,
            no_relay,
            fanalab,
            write_ibt,
            port_offset,
        ),
        Cmd::Setup => run_setup(),
        Cmd::Install { mode } => run_install(mode),
        Cmd::Uninstall => run_uninstall(),
        Cmd::List { verbose } => run_list(verbose),
        Cmd::Firewall => run_firewall(),
        Cmd::Stub => loop {
            std::thread::sleep(std::time::Duration::from_secs(3600));
        },
        Cmd::RegSetup => run_reg_setup(),
    }
}

#[allow(clippy::too_many_arguments)]
fn run_source(
    target: Option<String>,
    bind: Option<String>,
    unicast: bool,
    high_priority: bool,
    busy_wait: bool,
    iracing_port: Option<u16>,
    ac_port: Option<u16>,
    no_iracing: bool,
    no_ac: bool,
    no_relay: bool,
    scan_interval: Option<u64>,
    drain: Option<u64>,
    verbose: bool,
    port_offset: Option<u16>,
    no_cpu_exclude: bool,
) {
    if no_cpu_exclude {
        eprintln!("[cpu] CPU 0 exclusion disabled by --no-cpu-exclude flag");
    } else {
        cpu::avoid_cpu0();
    }
    let log = logger::Logger::open().unwrap_or_else(|e| {
        eprintln!("Warning: could not open log file: {e}");
        logger::Logger::stderr()
    });
    log.log(&format!(
        "sim-teleport v{VERSION} — source (teleport={TELEPORT_VERSION}, ac={AC_VERSION}, relay={RELAY_VERSION})"
    ));

    // Priority: CLI flags > toml > built-in defaults.
    let mut cfg = config::try_load().unwrap_or_default();

    if let Some(t) = target {
        cfg.network.target_ip = t;
    }
    if let Some(b) = bind {
        cfg.network.source_ip = b;
    }
    if unicast {
        cfg.network.unicast = true;
    }
    if high_priority {
        cfg.apps.high_priority = true;
    }
    if busy_wait {
        cfg.apps.busy_wait = true;
    }
    if let Some(p) = iracing_port {
        cfg.ports.iracing_teleport = p;
    }
    if let Some(p) = ac_port {
        cfg.ports.ac_teleport = p;
    }
    if no_iracing {
        cfg.apps.iracing_teleport_enabled = false;
    }
    if no_ac {
        cfg.apps.ac_teleport_enabled = false;
    }
    if no_relay {
        cfg.apps.sim_relay_enabled = false;
    }
    if let Some(s) = scan_interval {
        cfg.detection.scan_interval = s;
    }
    if let Some(d) = drain {
        cfg.detection.drain_seconds = d;
    }
    if verbose {
        cfg.verbose = true;
    }
    if let Some(o) = port_offset {
        cfg.apps.relay_port_offset = o;
    }

    let version_string = format!(
        "{VERSION} (teleport {TELEPORT_VERSION}, ac-teleport {AC_VERSION}, sim-relay {RELAY_VERSION})"
    );

    let (tx, rx) = mpsc::channel::<()>();
    ctrlc::set_handler(move || {
        println!("\nCtrl-C received...");
        let _ = tx.send(());
    })
    .expect("failed to install Ctrl-C handler");

    source::run(cfg, &log, rx, &version_string);
}

#[allow(clippy::too_many_arguments)]
fn run_target(
    source: Option<String>,
    unicast: bool,
    high_priority: bool,
    busy_wait: bool,
    iracing_port: Option<u16>,
    ac_port: Option<u16>,
    no_iracing: bool,
    no_ac: bool,
    no_relay: bool,
    fanalab: bool,
    write_ibt: bool,
    port_offset: Option<u16>,
) {
    let log = logger::Logger::open().unwrap_or_else(|e| {
        eprintln!("Warning: could not open log file: {e}");
        logger::Logger::stderr()
    });
    log.log(&format!(
        "sim-teleport v{VERSION} — target (teleport={TELEPORT_VERSION}, ac={AC_VERSION}, relay={RELAY_VERSION})"
    ));

    // Priority: CLI flags > toml > built-in defaults.
    let mut cfg = config::try_load().unwrap_or_default();

    if let Some(s) = source {
        cfg.network.source_ip = s;
    }
    if unicast {
        cfg.network.unicast = true;
    }
    if high_priority {
        cfg.apps.high_priority = true;
    }
    if busy_wait {
        cfg.apps.busy_wait = true;
    }
    if let Some(p) = iracing_port {
        cfg.ports.iracing_teleport = p;
    }
    if let Some(p) = ac_port {
        cfg.ports.ac_teleport = p;
    }
    if no_iracing {
        cfg.apps.iracing_teleport_enabled = false;
    }
    if no_ac {
        cfg.apps.ac_teleport_enabled = false;
    }
    if no_relay {
        cfg.apps.sim_relay_enabled = false;
    }
    if fanalab {
        cfg.apps.fanalab = true;
    }
    if write_ibt {
        cfg.apps.write_ibt = true;
    }
    if let Some(o) = port_offset {
        cfg.apps.relay_port_offset = o;
    }

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
        Ok(cfg) => {
            if cfg.apps.ac_teleport_enabled {
                let stub_dir = std::env::temp_dir().join("sim-teleport-stubs");
                std::fs::create_dir_all(&stub_dir).ok();
                let log = logger::Logger::stderr();
                stub::setup_all_game_environments(&stub_dir, &log);
                let steam_libs = steam::find_steam_libraries(&|s| log.log(s));
                steam::ensure_ac_appmanifests(&steam_libs, &stub_dir, &|s| log.log(s));
            }
        }
        Err(e) => {
            eprintln!("Setup failed: {e}");
            std::process::exit(1);
        }
    }
}

fn run_reg_setup() {
    eprintln!(
        "The reg-setup command is no longer needed. \
         SimHub AC detection now uses Steam appmanifests."
    );
}

fn run_install(mode_arg: Option<String>) {
    let mode = mode_arg.unwrap_or_else(|| {
        config::try_load()
            .map(|c| c.mode)
            .unwrap_or_else(|| "source".to_string())
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

fn run_list(verbose: bool) {
    let cfg = config::try_load().unwrap_or_default();
    println!("sim-teleport — Supported Games");
    println!();
    if verbose {
        println!("Shared Memory (auto-detected, started by sim-teleport source):");
        println!("  {:<35} {:<40} Port", "Game", "Detection");
        println!("  {}", "-".repeat(85));
        println!(
            "  {:<35} {:<40} {}",
            "iRacing", "Process scan: iRacingSim64DX11.exe", cfg.ports.iracing_teleport
        );
        println!(
            "  {:<35} {:<40} {}",
            "Assetto Corsa EVO", "Shmem probe: acevo_pmf_physics", cfg.ports.ac_teleport
        );
        println!(
            "  {:<35} {:<40} {}",
            "Assetto Corsa", "Shmem probe: acpmf_physics", cfg.ports.ac_teleport
        );
        println!(
            "  {:<35} {:<40} {}",
            "Assetto Corsa Competizione",
            "Shmem probe: acpmf_physics + acc.exe",
            cfg.ports.ac_teleport
        );
        println!();
        println!("UDP Relay (process scan, started on demand):");
        println!("  {:<35} {:<8} Process names", "Game", "Port");
        println!("  {}", "-".repeat(85));
        for game in sim_relay::games::GAMES {
            let port = game.default_port;
            let names = game.process_names.join(", ");
            println!("  {:<35} {:<8} {}", game.name, port, names);
        }
    } else {
        println!("Shared Memory (auto-detected, started by sim-teleport source):");
        println!("  {:<35} {:<35} Port", "Game", "Process");
        println!("  {}", "-".repeat(80));
        println!(
            "  {:<35} {:<35} {}",
            "iRacing", "iRacingSim64DX11.exe", cfg.ports.iracing_teleport
        );
        println!(
            "  {:<35} {:<35} {}",
            "Assetto Corsa EVO", "AssettoCorsa_EVO.exe", cfg.ports.ac_teleport
        );
        println!(
            "  {:<35} {:<35} {}",
            "Assetto Corsa", "acs.exe", cfg.ports.ac_teleport
        );
        println!(
            "  {:<35} {:<35} {}",
            "Assetto Corsa Competizione", "acc.exe", cfg.ports.ac_teleport
        );
        println!();
        println!("UDP Relay (process scan, started on demand):");
        for game in sim_relay::games::GAMES {
            let port = game.default_port;
            println!("  {:<35} port {port}", game.name);
        }
    }
}

fn run_firewall() {
    let cfg = config::try_load().unwrap_or_default();
    let iracing_port = cfg.ports.iracing_teleport;
    let ac_port = cfg.ports.ac_teleport;
    let offset = cfg.apps.relay_port_offset;

    // Target PC receives sim-relay traffic on the offset ports.
    // Games whose port + offset overflows u16 are excluded (same as the runtime skip).
    let mut relay_ports: Vec<u16> = sim_relay::games::GAMES
        .iter()
        .filter_map(|g| g.default_port.checked_add(offset))
        .collect();
    relay_ports.sort_unstable();
    relay_ports.dedup();
    let relay_port_list = relay_ports
        .iter()
        .map(|p| p.to_string())
        .collect::<Vec<_>>()
        .join(",");

    println!("# Run the following in PowerShell (Administrator).");
    if offset != 0 {
        println!("# Using port offset {offset}: sim-relay traffic arrives on game_port+{offset}.");
    }
    println!("# Gaming PC (receives resync packets from SimHub PC):");
    println!();
    println!("New-NetFirewallRule -DisplayName \"sim-teleport source\" `");
    println!(
        "    -Direction Inbound -Protocol UDP -LocalPort {iracing_port},{ac_port} -Action Allow"
    );
    println!();
    println!("# SimHub PC (receives telemetry from gaming PC):");
    println!();
    println!("New-NetFirewallRule -DisplayName \"sim-teleport target\" `");
    println!(
        "    -Direction Inbound -Protocol UDP -LocalPort {iracing_port},{ac_port},{relay_port_list} -Action Allow"
    );
}

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::io::{self, Write};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    #[serde(default = "default_mode")]
    pub mode: String,
    pub network: NetworkConfig,
    pub ports: PortsConfig,
    pub detection: DetectionConfig,
    pub apps: AppsConfig,
    #[serde(default)]
    pub advanced: AdvancedConfig,
    #[serde(default)]
    pub simhub: SimhubConfig,
    /// CLI-only flag — never written to or read from toml.
    #[serde(skip)]
    pub verbose: bool,
}

fn default_mode() -> String {
    "source".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkConfig {
    pub source_ip: String,
    pub target_ip: String,
    #[serde(default)]
    pub unicast: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PortsConfig {
    pub iracing_teleport: u16,
    pub ac_teleport: u16,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DetectionConfig {
    pub scan_interval: u64,
    pub drain_seconds: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppsConfig {
    pub iracing_teleport_enabled: bool,
    pub ac_teleport_enabled: bool,
    pub sim_relay_enabled: bool,
    #[serde(default)]
    pub high_priority: bool,
    #[serde(default)]
    pub busy_wait: bool,
    #[serde(default)]
    pub fanalab: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdvancedConfig {
    #[serde(default = "default_stale_timeout")]
    pub stale_timeout_secs: u64,
    #[serde(default = "default_reconnect_timeout")]
    pub reconnect_timeout_secs: u64,
    #[serde(default = "default_ac_poll_rate")]
    pub ac_poll_rate: u32,
    #[serde(default = "default_datagram_size")]
    pub datagram_size: usize,
}

fn default_stale_timeout() -> u64 {
    10
}
fn default_reconnect_timeout() -> u64 {
    10
}
fn default_ac_poll_rate() -> u32 {
    60
}
fn default_datagram_size() -> usize {
    9000
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SimhubConfig {
    /// Full path to SimHubWPF.exe. None = use the default install location.
    pub path: Option<String>,
    /// SimHub game code for iRacing telemetry.
    #[serde(default = "default_iracing_code")]
    pub iracing: String,
    /// SimHub game code for AC/ACE/ACC telemetry.
    #[serde(default = "default_ac_code")]
    pub ac: String,
    /// Maps sim-relay game IDs to SimHub game codes.
    /// Example: wreckfest2 = "Wreckfest2"
    #[serde(default)]
    pub relay: HashMap<String, String>,
}

fn default_iracing_code() -> String {
    "iRacing".to_string()
}
fn default_ac_code() -> String {
    "AssettoCorsa".to_string()
}

impl Default for SimhubConfig {
    fn default() -> Self {
        Self {
            path: None,
            iracing: default_iracing_code(),
            ac: default_ac_code(),
            relay: HashMap::new(),
        }
    }
}

impl Default for AdvancedConfig {
    fn default() -> Self {
        Self {
            stale_timeout_secs: default_stale_timeout(),
            reconnect_timeout_secs: default_reconnect_timeout(),
            ac_poll_rate: default_ac_poll_rate(),
            datagram_size: default_datagram_size(),
        }
    }
}

impl Default for Config {
    fn default() -> Self {
        Self {
            mode: "source".to_string(),
            verbose: false,
            network: NetworkConfig {
                source_ip: "192.168.50.1".to_string(),
                target_ip: "192.168.50.2".to_string(),
                unicast: false,
            },
            ports: PortsConfig {
                iracing_teleport: 5000,
                ac_teleport: 5001,
            },
            detection: DetectionConfig {
                scan_interval: 3,
                drain_seconds: 20,
            },
            apps: AppsConfig {
                iracing_teleport_enabled: true,
                ac_teleport_enabled: true,
                sim_relay_enabled: true,
                high_priority: false,
                busy_wait: false,
                fanalab: false,
            },
            advanced: AdvancedConfig::default(),
            simhub: SimhubConfig::default(),
        }
    }
}

const CONFIG_FILENAME: &str = "sim-bridge.toml";
const APPDATA_DIR: &str = "sim-bridge";

/// Load config from disk without triggering the wizard. Returns None if no file exists.
pub fn try_load() -> Option<Config> {
    let path = find_config()?;
    let text = fs::read_to_string(&path).ok()?;
    toml::from_str(&text).ok()
}

pub fn write_config(config: &Config, path: &PathBuf) -> anyhow::Result<()> {
    let toml_text = format!(
        r#"# sim-bridge configuration
# Place next to sim-bridge.exe, or at %APPDATA%\sim-bridge\sim-bridge.toml

# PC role: "source" (gaming PC) or "target" (SimHub PC)
mode = "{mode}"

[network]
# Set true for direct ethernet (point-to-point, no switch/router).
# Leave false for regular LAN (uses multicast, no IP config needed).
unicast = {unicast}
# IPs are only used when unicast = true (iRacing/AC) or for sim-relay forwarding.
source_ip = "{source_ip}"
target_ip = "{target_ip}"

[ports]
# Each shared-memory app uses a different port
iracing_teleport = {iracing_port}
ac_teleport = {ac_port}
# sim-relay uses native game ports (20777, 5606, etc.)

[detection]
# Process scan interval in seconds (source only)
scan_interval = {scan_interval}
# Grace period before stopping an app after game closes
drain_seconds = {drain_seconds}

[apps]
# Set false to disable an app entirely
iracing_teleport_enabled = {iracing_enabled}
ac_teleport_enabled = {ac_enabled}
sim_relay_enabled = {relay_enabled}
# Performance options
high_priority = {high_priority}
busy_wait = {busy_wait}
# Enable FanaLab shared-memory output on the target PC
fanalab = {fanalab}

[advanced]
# Seconds before target marks stale iRacing/AC data as dead
stale_timeout_secs = {stale_timeout}
# Seconds iRacing source waits for reconnect before resetting
reconnect_timeout_secs = {reconnect_timeout}
# AC Teleport source poll rate (Hz)
ac_poll_rate = {ac_poll_rate}
# iRacing Teleport datagram size in bytes
datagram_size = {datagram_size}

[simhub]
# Automatically switch SimHub to the correct game when telemetry arrives.
# Full path to SimHubWPF.exe (leave commented for default install location).
# path = "C:/Program Files (x86)/SimHub/SimHubWPF.exe"
# SimHub game codes — change only if SimHub uses a different string for your setup.
iracing = "{iracing_code}"
ac = "{ac_code}"
"#,
        mode = config.mode,
        unicast = config.network.unicast,
        source_ip = config.network.source_ip,
        target_ip = config.network.target_ip,
        iracing_port = config.ports.iracing_teleport,
        ac_port = config.ports.ac_teleport,
        scan_interval = config.detection.scan_interval,
        drain_seconds = config.detection.drain_seconds,
        iracing_enabled = config.apps.iracing_teleport_enabled,
        ac_enabled = config.apps.ac_teleport_enabled,
        relay_enabled = config.apps.sim_relay_enabled,
        high_priority = config.apps.high_priority,
        busy_wait = config.apps.busy_wait,
        fanalab = config.apps.fanalab,
        stale_timeout = config.advanced.stale_timeout_secs,
        reconnect_timeout = config.advanced.reconnect_timeout_secs,
        ac_poll_rate = config.advanced.ac_poll_rate,
        datagram_size = config.advanced.datagram_size,
        iracing_code = config.simhub.iracing,
        ac_code = config.simhub.ac,
    );
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, toml_text)?;
    Ok(())
}

fn find_config() -> Option<PathBuf> {
    let exe_path = exe_dir_config();
    if exe_path.exists() {
        return Some(exe_path);
    }
    if let Some(appdata) = appdata_config() {
        if appdata.exists() {
            return Some(appdata);
        }
    }
    None
}

fn exe_dir_config() -> PathBuf {
    std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|d| d.join(CONFIG_FILENAME)))
        .unwrap_or_else(|| PathBuf::from(CONFIG_FILENAME))
}

fn appdata_config() -> Option<PathBuf> {
    std::env::var("APPDATA").ok().map(|appdata| {
        PathBuf::from(appdata)
            .join(APPDATA_DIR)
            .join(CONFIG_FILENAME)
    })
}

pub fn setup_wizard() -> anyhow::Result<Config> {
    let mut config = Config::default();

    println!("=== sim-bridge setup ===");
    println!();

    // Connection type — determines whether unicast is needed.
    println!("How are the two PCs connected?");
    println!("  [1] LAN — both PCs on the same network (zero config, recommended)");
    println!("  [2] Direct ethernet — dedicated cable between the two PCs");
    print!("> ");
    io::stdout().flush()?;
    let conn = read_line();
    config.network.unicast = conn == "2";

    // Mode
    println!();
    println!("Is this the gaming PC or the SimHub PC?");
    println!("  [1] Source — games run here (default)");
    println!("  [2] Target — SimHub runs here");
    print!("> ");
    io::stdout().flush()?;
    let input = read_line();
    config.mode = if input == "2" {
        "target".to_string()
    } else {
        "source".to_string()
    };

    // IP addresses — only needed for direct ethernet (unicast) or sim-relay forwarding.
    if config.network.unicast {
        println!();
        if config.mode == "source" {
            print!("This PC's IP (gaming PC): [{}] ", config.network.source_ip);
            io::stdout().flush()?;
            let input = read_line();
            if !input.is_empty() {
                config.network.source_ip = input;
            }

            print!("Remote SimHub PC's IP: [{}] ", config.network.target_ip);
            io::stdout().flush()?;
            let input = read_line();
            if !input.is_empty() {
                config.network.target_ip = input;
            }
        } else {
            print!("Remote gaming PC's IP: [{}] ", config.network.source_ip);
            io::stdout().flush()?;
            let input = read_line();
            if !input.is_empty() {
                config.network.source_ip = input;
            }

            print!("This PC's IP (SimHub PC): [{}] ", config.network.target_ip);
            io::stdout().flush()?;
            let input = read_line();
            if !input.is_empty() {
                config.network.target_ip = input;
            }
        }
    } else if config.apps.sim_relay_enabled {
        // Sim Relay always needs the target IP for UDP forwarding even on LAN.
        println!();
        println!("Sim Relay forwards UDP to the SimHub PC — enter its IP.");
        println!("(Skip this if you don't use F1, Forza, BeamNG, etc.)");
        print!(
            "SimHub PC's IP (for Sim Relay): [{}] ",
            config.network.target_ip
        );
        io::stdout().flush()?;
        let input = read_line();
        if !input.is_empty() {
            config.network.target_ip = input;
        }
    }

    println!();
    println!("Which apps to enable? (press Enter to keep all enabled)");
    println!("  [x] iRacing Teleport (iRacing)");
    println!("  [x] AC Teleport (Assetto Corsa, AC EVO, ACC)");
    println!("  [x] Sim Relay (F1, Forza, PCars, BeamNG, Wreckfest, etc.)");
    print!("> disable any? (iracing/ac/relay, comma-separated, or Enter to keep all): ");
    io::stdout().flush()?;
    let input = read_line();
    for item in input.split(',').map(|s| s.trim().to_lowercase()) {
        match item.as_str() {
            "iracing" => config.apps.iracing_teleport_enabled = false,
            "ac" => config.apps.ac_teleport_enabled = false,
            "relay" => config.apps.sim_relay_enabled = false,
            _ => {}
        }
    }

    println!();
    println!("Tip: Place sim-bridge.exe in a user-writable folder like C:\\Simracing\\");
    println!("     (not in Program Files). The log and config are written next to the exe.");

    let save_path = exe_dir_config();
    write_config(&config, &save_path)?;
    println!();
    println!("Config saved to {}", save_path.display());
    if !config.network.unicast {
        println!();
        println!("LAN mode: no IP configuration needed for iRacing and AC.");
        println!("Both PCs must be on the same network.");
    }
    println!();
    println!("To start now:     sim-bridge {}", config.mode);
    println!("To start on boot: sim-bridge install");

    Ok(config)
}

fn read_line() -> String {
    let mut buf = String::new();
    let _ = std::io::stdin().read_line(&mut buf);
    buf.trim().to_string()
}

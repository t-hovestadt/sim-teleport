use serde::{Deserialize, Serialize};
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
}

fn default_mode() -> String {
    "source".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkConfig {
    pub source_ip: String,
    pub target_ip: String,
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
}

impl Default for Config {
    fn default() -> Self {
        Self {
            mode: "source".to_string(),
            network: NetworkConfig {
                source_ip: "192.168.50.1".to_string(),
                target_ip: "192.168.50.2".to_string(),
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
            },
        }
    }
}

const CONFIG_FILENAME: &str = "sim-bridge.toml";
const APPDATA_DIR: &str = "sim-bridge";

pub fn load() -> anyhow::Result<Config> {
    match find_config() {
        Some(path) => {
            let text = fs::read_to_string(&path)?;
            let config: Config = toml::from_str(&text)
                .map_err(|e| anyhow::anyhow!("config parse error: {e}"))?;
            Ok(config)
        }
        None => {
            println!("No config found. Running setup wizard...");
            println!();
            setup_wizard()
        }
    }
}

pub fn write_config(config: &Config, path: &PathBuf) -> anyhow::Result<()> {
    let toml_text = format!(
        r#"# sim-bridge configuration
# Place next to sim-bridge.exe, or at %APPDATA%\sim-bridge\sim-bridge.toml

# PC role: "source" (gaming PC) or "target" (SimHub PC)
mode = "{mode}"

[network]
# Source PC IP (gaming PC where games run)
source_ip = "{source_ip}"
# Target PC IP (SimHub PC)
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
"#,
        mode = config.mode,
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

    // Mode
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

    print!("This PC's IP (source/gaming PC): [{}] ", config.network.source_ip);
    io::stdout().flush()?;
    let input = read_line();
    if !input.is_empty() {
        config.network.source_ip = input;
    }

    print!("Target PC's IP (SimHub PC): [{}] ", config.network.target_ip);
    io::stdout().flush()?;
    let input = read_line();
    if !input.is_empty() {
        config.network.target_ip = input;
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

    let save_path = exe_dir_config();
    write_config(&config, &save_path)?;
    println!();
    println!("Config saved to {}", save_path.display());
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

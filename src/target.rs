use std::sync::mpsc::{self, Receiver};
use std::thread::JoinHandle;

use crate::config::Config;
use crate::logger::Logger;

pub fn run(config: Config, log: &Logger, shutdown: Receiver<()>) {
    log.log("Listening:");
    log.log(&format!("  iRacing Teleport  :{}", config.ports.iracing_teleport));
    log.log(&format!("  AC Teleport       :{}", config.ports.ac_teleport));
    log.log("  Sim Relay          all game ports");
    log.log("Waiting for telemetry...");

    let (tx1, rx1) = mpsc::channel::<()>();
    let (tx2, rx2) = mpsc::channel::<()>();
    let (tx3, rx3) = mpsc::channel::<()>();

    let t1 = spawn_teleport_target(&config, rx1);
    let t2 = spawn_ac_target(&config, rx2);
    let t3 = spawn_relay_target(&config, rx3);

    let _ = shutdown.recv();

    log.log("Shutting down...");
    let _ = tx1.send(());
    let _ = tx2.send(());
    let _ = tx3.send(());
    let _ = t1.join();
    let _ = t2.join();
    let _ = t3.join();
    log.log("All apps stopped. Goodbye.");
}

fn spawn_teleport_target(config: &Config, rx: Receiver<()>) -> JoinHandle<()> {
    let cfg = config.clone();
    std::thread::Builder::new()
        .name("iRacing Teleport Target".to_string())
        .spawn(move || {
            if let Err(e) = teleport::run_target(
                teleport::TargetConfig {
                    bind: format!("0.0.0.0:{}", cfg.ports.iracing_teleport),
                    unicast: true,
                    ..teleport::TargetConfig::default()
                },
                rx,
            ) {
                eprintln!("[iRacing Teleport Target] {e}");
            }
        })
        .expect("failed to spawn iRacing Teleport target thread")
}

fn spawn_ac_target(config: &Config, rx: Receiver<()>) -> JoinHandle<()> {
    let cfg = config.clone();
    std::thread::Builder::new()
        .name("AC Teleport Target".to_string())
        .spawn(move || {
            // game: None = dual mode — creates shared maps for both EVO and AC1
            // simultaneously, so the target handles any AC variant without restart.
            if let Err(e) = ac_teleport::target::run(
                ac_teleport::TargetArgs {
                    game: None,
                    bind: format!("0.0.0.0:{}", cfg.ports.ac_teleport),
                    group: teleport::DEFAULT_MULTICAST.to_string(),
                    unicast: true,
                    busy_wait: false,
                    pin_core: None,
                    high_priority: false,
                    stale_timeout: std::time::Duration::from_secs(10),
                },
                rx,
            ) {
                eprintln!("[AC Teleport Target] {e}");
            }
        })
        .expect("failed to spawn AC Teleport target thread")
}

fn spawn_relay_target(config: &Config, rx: Receiver<()>) -> JoinHandle<()> {
    let cfg = config.clone();
    std::thread::Builder::new()
        .name("Sim Relay Target".to_string())
        .spawn(move || {
            if let Err(e) = sim_relay::target::run(
                sim_relay::TargetArgs {
                    source: Some(cfg.network.source_ip.clone()),
                    games: None,
                    all: true,
                    forward_to: None,
                    high_priority: false,
                    busy_wait: false,
                },
                rx,
            ) {
                eprintln!("[Sim Relay Target] {e}");
            }
        })
        .expect("failed to spawn Sim Relay target thread")
}

use std::io::{self, ErrorKind};
use std::net::{SocketAddr, UdpSocket};
use std::sync::mpsc;
use std::time::{Duration, Instant};

use clap::Parser;
use socket2::{Domain, Protocol, Socket, Type};

use crate::games::{Detection, GameDef};
use crate::platform::{boost_thread_priority, is_process_running, set_high_priority, HighResTimer};
use crate::stats::RelayStats;

/// Capture game telemetry on this PC and forward to a target PC running SimHub.
#[derive(Parser)]
#[command(name = "source", version, about)]
pub struct Args {
    /// Target PC IP address
    #[arg(long, value_name = "IP")]
    pub target: String,
    /// Comma-separated game IDs to forward (default: all)
    #[arg(long, value_name = "ID,...", value_delimiter = ',')]
    pub games: Option<Vec<String>>,
    /// Forward all supported games
    #[arg(long)]
    pub all: bool,
    /// Also forward to localhost:<port+1000> for a local SimHub instance
    #[arg(long)]
    pub local_forward: bool,
    /// Bind address for listening sockets
    #[arg(long, default_value = "0.0.0.0")]
    pub bind: String,
    /// Set HIGH_PRIORITY_CLASS for this process
    #[arg(long)]
    pub high_priority: bool,
    /// Only bind ports when the game process is detected running
    #[arg(long)]
    pub auto_detect: bool,
}

struct GameRelay {
    def: &'static GameDef,
    socket: Option<UdpSocket>,
    target_addr: SocketAddr,
    local_addr: Option<SocketAddr>,
    stats: RelayStats,
    last_packet: Option<Instant>,
    active: bool,
    last_process_check: Instant,
    process_running: bool,
}

fn bind_socket(port: u16, bind_ip: &str) -> io::Result<UdpSocket> {
    let addr: SocketAddr = format!("{bind_ip}:{port}").parse().map_err(|e| {
        io::Error::new(
            ErrorKind::InvalidInput,
            format!("invalid bind address: {e}"),
        )
    })?;
    let sock = Socket::new(Domain::IPV4, Type::DGRAM, Some(Protocol::UDP))?;
    sock.set_reuse_address(true)?;
    sock.set_send_buffer_size(512 * 1024)?;
    sock.bind(&addr.into())?;
    let udp: UdpSocket = sock.into();
    udp.set_nonblocking(true)?;
    Ok(udp)
}

pub fn run(args: Args, shutdown: mpsc::Receiver<()>) -> io::Result<()> {
    if args.high_priority {
        set_high_priority();
        boost_thread_priority();
    }
    let _timer = HighResTimer::acquire();

    let selected = crate::games::select_games(&args.games, args.all)?;
    if selected.is_empty() {
        eprintln!("No games selected. Use --games <id,...> or --all.");
        return Ok(());
    }

    let target_ip = args.target.as_str();

    let mut relays: Vec<GameRelay> = Vec::new();
    for def in selected {
        let target_addr: SocketAddr = format!("{target_ip}:{}", def.default_port)
            .parse()
            .map_err(|e| {
                io::Error::new(
                    ErrorKind::InvalidInput,
                    format!("invalid target '{}': {e}", args.target),
                )
            })?;

        let local_addr = if args.local_forward {
            let local_port = def.default_port.saturating_add(1000);
            Some(
                format!("127.0.0.1:{local_port}")
                    .parse::<SocketAddr>()
                    .unwrap(),
            )
        } else {
            None
        };

        let (socket, process_running) = if args.auto_detect {
            // Start unbound; will bind when the game process is detected.
            (None, false)
        } else {
            let s = bind_socket(def.default_port, &args.bind).map_err(|e| {
                io::Error::new(
                    e.kind(),
                    format!(
                        "[{}] failed to bind port {}: {e}",
                        def.name, def.default_port
                    ),
                )
            })?;
            println!(
                "[{}] listening on {}:{} → {target_addr}",
                def.name, args.bind, def.default_port
            );
            (Some(s), true)
        };

        relays.push(GameRelay {
            def,
            socket,
            target_addr,
            local_addr,
            stats: RelayStats::new(),
            last_packet: None,
            active: false,
            // Subtract 6s so the first process-check fires immediately on the first loop iteration.
            last_process_check: Instant::now() - Duration::from_secs(6),
            process_running,
        });
    }

    if args.auto_detect {
        println!(
            "Auto-detect enabled: ports will bind only when the game process is detected running."
        );
    }
    println!("Forwarding to {} | Ctrl-C to stop", args.target);

    let mut buf = [0u8; 65_536];
    let mut last_stats = Instant::now();
    let start = Instant::now();

    loop {
        if shutdown.try_recv().is_ok() {
            break;
        }

        for relay in &mut relays {
            if args.auto_detect && relay.last_process_check.elapsed() >= Duration::from_secs(5) {
                let running = match &relay.def.detection {
                    Detection::Process(names) => is_process_running(names),
                    Detection::UdpActivity => relay.socket.is_some(),
                };
                if running && relay.socket.is_none() {
                    match bind_socket(relay.def.default_port, &args.bind) {
                        Ok(s) => {
                            println!(
                                "[{}] process detected — bound port {}",
                                relay.def.name, relay.def.default_port
                            );
                            relay.socket = Some(s);
                        }
                        Err(e) => {
                            eprintln!(
                                "[{}] failed to bind port {}: {e}",
                                relay.def.name, relay.def.default_port
                            );
                        }
                    }
                } else if !running && relay.socket.is_some() {
                    relay.socket = None;
                    relay.active = false;
                    println!(
                        "[{}] process gone — unbound port {}",
                        relay.def.name, relay.def.default_port
                    );
                }
                relay.process_running = running;
                relay.last_process_check = Instant::now();
            }

            let Some(ref socket) = relay.socket else {
                continue;
            };

            loop {
                match socket.recv_from(&mut buf) {
                    Ok((len, _src)) => {
                        let t0 = Instant::now();
                        let _ = socket.send_to(&buf[..len], relay.target_addr);
                        if let Some(local) = relay.local_addr {
                            let _ = socket.send_to(&buf[..len], local);
                        }
                        let fwd_us = t0.elapsed().as_micros() as u64;
                        relay.stats.record(len, fwd_us);
                        if !relay.active {
                            relay.active = true;
                            println!(
                                "[{}] data flowing \u{2192} {}",
                                relay.def.name, relay.target_addr
                            );
                        }
                        relay.last_packet = Some(Instant::now());
                    }
                    Err(e) if e.kind() == ErrorKind::WouldBlock => break,
                    Err(e) => {
                        eprintln!("[{}] recv error: {e}", relay.def.id);
                        break;
                    }
                }
            }

            if relay.active
                && relay
                    .last_packet
                    .is_none_or(|t| t.elapsed() > Duration::from_secs(10))
            {
                relay.active = false;
                println!(
                    "[{}] no data for 10 s — game likely paused or closed",
                    relay.def.name
                );
            }
        }

        if last_stats.elapsed() >= Duration::from_secs(5) {
            for relay in &mut relays {
                relay.stats.maybe_print(relay.def.name, relay.active);
            }
            last_stats = Instant::now();
        }

        std::thread::sleep(Duration::from_micros(100));
    }

    let elapsed = start.elapsed();
    println!("\n--- Summary ---");
    for relay in &relays {
        relay.stats.print_summary(relay.def.name, elapsed);
    }
    Ok(())
}

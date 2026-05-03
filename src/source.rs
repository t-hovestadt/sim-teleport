use std::io::{self, ErrorKind};
use std::net::{SocketAddr, UdpSocket};
use std::sync::mpsc;
use std::time::{Duration, Instant};

use clap::Parser;
use socket2::{Domain, Protocol, Socket, Type};

use crate::games::PortGroup;
use crate::platform::{boost_thread_priority, set_high_priority, HighResTimer, ProcessScanner};
use crate::stats::RelayStats;

/// Capture game telemetry on this PC and forward to a target PC running SimHub.
#[derive(Parser)]
#[command(name = "source", version, about)]
pub struct Args {
    /// Target PC IP address
    #[arg(long, value_name = "IP")]
    pub target: String,
    /// Comma-separated game IDs to forward (default: auto-detect all)
    #[arg(long, value_name = "ID,...", value_delimiter = ',')]
    pub games: Option<Vec<String>>,
    /// Bind all ports immediately, skip process detection
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
    /// How often to scan for game processes (seconds)
    #[arg(long, default_value = "5", value_name = "SECS")]
    pub scan_interval: u64,
    /// How long to keep forwarding after a game exits (seconds)
    #[arg(long, default_value = "15", value_name = "SECS")]
    pub grace_period: u64,
    /// Include console-only games (GT7, GT Sport) in auto-detect mode
    #[arg(long)]
    pub include_console: bool,
    /// Bind all ports immediately, skip process detection (alias for --all)
    #[arg(long)]
    pub force_bind: bool,
}

/// Phase of a relay — tracks detection/drain timing. Socket is kept separately.
#[derive(Debug)]
enum RelayPhase {
    Idle,
    Active,
    Draining { since: Instant },
}

struct ManagedRelay {
    group: PortGroup,
    socket: Option<UdpSocket>,
    phase: RelayPhase,
    target_addr: SocketAddr,
    local_addr: Option<SocketAddr>,
    stats: RelayStats,
    last_packet: Option<Instant>,
    first_packet: bool,
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

    let immediate_bind = args.all || args.force_bind;
    let explicit_games = args.games.as_ref().is_some_and(|v| !v.is_empty());

    let selected = if explicit_games {
        crate::games::select_games(&args.games, false)?
    } else {
        crate::games::select_games(&None, true)?
    };

    if selected.is_empty() {
        eprintln!("No games selected.");
        return Ok(());
    }

    let target_ip = args.target.as_str();
    let scan_interval = Duration::from_secs(args.scan_interval);
    let grace_period = Duration::from_secs(args.grace_period);

    let mut relays: Vec<ManagedRelay> = Vec::new();
    for group in selected {
        let target_addr: SocketAddr =
            format!("{target_ip}:{}", group.port).parse().map_err(|e| {
                io::Error::new(
                    ErrorKind::InvalidInput,
                    format!("invalid target '{}': {e}", args.target),
                )
            })?;

        let local_addr = if args.local_forward {
            let local_port = group.port.saturating_add(1000);
            Some(
                format!("127.0.0.1:{local_port}")
                    .parse::<SocketAddr>()
                    .unwrap(),
            )
        } else {
            None
        };

        let (socket, phase) = if immediate_bind || explicit_games {
            match bind_socket(group.port, &args.bind) {
                Ok(s) => {
                    println!(
                        "[{}] listening on {}:{} \u{2192} {target_addr}",
                        group.display_name, args.bind, group.port
                    );
                    (Some(s), RelayPhase::Active)
                }
                Err(e) => {
                    return Err(io::Error::new(
                        e.kind(),
                        format!(
                            "[{}] failed to bind port {}: {e}",
                            group.display_name, group.port
                        ),
                    ));
                }
            }
        } else {
            (None, RelayPhase::Idle)
        };

        relays.push(ManagedRelay {
            group,
            socket,
            phase,
            target_addr,
            local_addr,
            stats: RelayStats::new(),
            last_packet: None,
            first_packet: true,
        });
    }

    if immediate_bind || explicit_games {
        println!("Forwarding to {} | Ctrl-C to stop", args.target);
    } else {
        #[cfg(not(windows))]
        eprintln!(
            "Warning: process scanning is not available on this platform. \
             Use --all to bind immediately."
        );
        println!(
            "Auto-detect: scanning every {}s, grace period {}s | Ctrl-C to stop",
            args.scan_interval, args.grace_period
        );
    }

    let mut buf = [0u8; 65_536];
    let mut last_stats = Instant::now();
    // Pre-subtract scan_interval so the first scan fires on the first iteration.
    let mut last_scan = Instant::now() - scan_interval;
    let start = Instant::now();
    let mut scanner = ProcessScanner::new();

    loop {
        if shutdown.try_recv().is_ok() {
            break;
        }

        // ── Process scan (auto-detect mode only) ─────────────────────────────
        if !immediate_bind && !explicit_games && last_scan.elapsed() >= scan_interval {
            scanner.refresh();
            last_scan = Instant::now();

            for relay in &mut relays {
                if relay.group.console && !args.include_console {
                    continue;
                }

                let game_running = if relay.group.process_names.is_empty() {
                    // console game with --include-console: treat as always running
                    true
                } else {
                    scanner.is_running(&relay.group.process_names)
                };

                let current_phase = std::mem::replace(&mut relay.phase, RelayPhase::Idle);
                relay.phase = match current_phase {
                    RelayPhase::Idle if game_running => {
                        match bind_socket(relay.group.port, &args.bind) {
                            Ok(s) => {
                                println!(
                                    "[{}] detected \u{2014} binding port {}",
                                    relay.group.display_name, relay.group.port
                                );
                                relay.socket = Some(s);
                                relay.first_packet = true;
                                RelayPhase::Active
                            }
                            Err(e) => {
                                eprintln!(
                                    "[{}] failed to bind port {}: {e}",
                                    relay.group.display_name, relay.group.port
                                );
                                RelayPhase::Idle
                            }
                        }
                    }
                    RelayPhase::Active if !game_running => {
                        println!(
                            "[{}] exited \u{2014} draining for {}s",
                            relay.group.display_name, args.grace_period
                        );
                        RelayPhase::Draining {
                            since: Instant::now(),
                        }
                    }
                    RelayPhase::Draining { since: _ } if game_running => {
                        println!("[{}] returned \u{2014} resuming", relay.group.display_name);
                        relay.first_packet = true;
                        RelayPhase::Active
                    }
                    other => other,
                };
            }
        }

        // ── Grace-period expiry ───────────────────────────────────────────────
        for relay in &mut relays {
            if let RelayPhase::Draining { since } = relay.phase {
                if since.elapsed() > grace_period {
                    relay.socket = None;
                    relay.phase = RelayPhase::Idle;
                    println!(
                        "[{}] drain expired \u{2014} unbound port {}",
                        relay.group.display_name, relay.group.port
                    );
                }
            }
        }

        // ── Packet forwarding ─────────────────────────────────────────────────
        let mut any_active = false;
        for relay in &mut relays {
            let ManagedRelay {
                ref socket,
                ref mut stats,
                ref mut last_packet,
                ref target_addr,
                ref local_addr,
                ref group,
                ref mut first_packet,
                ..
            } = *relay;

            let sock = match socket {
                Some(s) => s,
                None => continue,
            };
            any_active = true;

            loop {
                match sock.recv_from(&mut buf) {
                    Ok((len, _src)) => {
                        let t0 = Instant::now();
                        let _ = sock.send_to(&buf[..len], *target_addr);
                        if let Some(local) = *local_addr {
                            let _ = sock.send_to(&buf[..len], local);
                        }
                        let fwd_us = t0.elapsed().as_micros() as u64;
                        stats.record(len, fwd_us);
                        if *first_packet {
                            *first_packet = false;
                            println!(
                                "[{}] data flowing \u{2192} {}",
                                group.display_name, target_addr
                            );
                        }
                        *last_packet = Some(Instant::now());
                    }
                    Err(e) if e.kind() == ErrorKind::WouldBlock => break,
                    Err(e) => {
                        eprintln!("[{}] recv error: {e}", group.id);
                        break;
                    }
                }
            }
        }

        // ── Stats ─────────────────────────────────────────────────────────────
        if last_stats.elapsed() >= Duration::from_secs(5) {
            for relay in &mut relays {
                let active = matches!(relay.phase, RelayPhase::Active);
                relay.stats.maybe_print(&relay.group.display_name, active);
            }
            last_stats = Instant::now();
        }

        if any_active {
            std::thread::sleep(Duration::from_micros(100));
        } else {
            std::thread::sleep(Duration::from_millis(100));
        }
    }

    let elapsed = start.elapsed();
    println!("\n--- Summary ---");
    for relay in &relays {
        relay
            .stats
            .print_summary(&relay.group.display_name, elapsed);
    }
    Ok(())
}

use std::io::{self, ErrorKind};
use std::net::{SocketAddr, UdpSocket};
use std::sync::mpsc;
use std::time::{Duration, Instant};

use socket2::{Domain, Protocol, Socket, Type};

use crate::games::GameDef;
use crate::platform::{boost_thread_priority, set_high_priority, HighResTimer};
use crate::stats::RelayStats;

pub struct Args {
    pub source: Option<String>,
    pub games: Option<Vec<String>>,
    pub all: bool,
    pub forward_to: Option<String>,
    pub high_priority: bool,
    pub busy_wait: bool,
}

struct GameReceiver {
    def: &'static GameDef,
    recv_socket: UdpSocket,
    fwd_socket: UdpSocket,
    forward_addr: SocketAddr,
    stats: RelayStats,
    last_packet: Option<Instant>,
    active: bool,
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

    let forward_override: Option<SocketAddr> = match &args.forward_to {
        Some(fwd) => Some(fwd.parse().map_err(|e| {
            io::Error::new(
                ErrorKind::InvalidInput,
                format!("invalid --forward-to '{fwd}': {e}"),
            )
        })?),
        None => None,
    };

    let mut receivers: Vec<GameReceiver> = Vec::new();
    for def in selected {
        let sock = Socket::new(Domain::IPV4, Type::DGRAM, Some(Protocol::UDP))?;
        sock.set_reuse_address(true)?;
        sock.set_recv_buffer_size(512 * 1024)?;
        let bind_addr: SocketAddr = format!("0.0.0.0:{}", def.default_port).parse().unwrap();
        sock.bind(&bind_addr.into())?;
        let recv_socket: UdpSocket = sock.into();
        recv_socket.set_nonblocking(true)?;

        // Separate socket for forwarding so the source port differs from the recv port,
        // preventing loopback packets from being re-received by this same socket.
        let fwd_socket = UdpSocket::bind("0.0.0.0:0")?;

        let forward_addr = forward_override
            .unwrap_or_else(|| format!("127.0.0.1:{}", def.default_port).parse().unwrap());

        println!(
            "[{}] listening on 0.0.0.0:{} → {forward_addr}",
            def.name, def.default_port
        );

        receivers.push(GameReceiver {
            def,
            recv_socket,
            fwd_socket,
            forward_addr,
            stats: RelayStats::new(),
            last_packet: None,
            active: false,
        });
    }

    if let Some(src) = &args.source {
        println!("Expecting traffic from {src} | Ctrl-C to stop");
    } else {
        println!("Ready | Ctrl-C to stop");
    }

    let mut buf = [0u8; 65_536];
    let mut last_stats = Instant::now();
    let start = Instant::now();

    loop {
        if shutdown.try_recv().is_ok() {
            break;
        }

        for recv in &mut receivers {
            match recv.recv_socket.recv_from(&mut buf) {
                Ok((len, _src)) => {
                    let _ = recv.fwd_socket.send_to(&buf[..len], recv.forward_addr);
                    recv.stats.record(len);
                    if !recv.active {
                        recv.active = true;
                        println!(
                            "[{}] traffic received \u{2192} {}",
                            recv.def.name, recv.forward_addr
                        );
                    }
                    recv.last_packet = Some(Instant::now());
                }
                Err(e) if e.kind() == ErrorKind::WouldBlock => {}
                Err(e) => eprintln!("[{}] recv error: {e}", recv.def.id),
            }

            if recv.active
                && recv
                    .last_packet
                    .is_none_or(|t| t.elapsed() > Duration::from_secs(10))
            {
                recv.active = false;
                println!("[{}] no data for 10 s", recv.def.name);
            }
        }

        if last_stats.elapsed() >= Duration::from_secs(5) {
            for recv in &mut receivers {
                recv.stats.maybe_print(recv.def.name, recv.active);
            }
            last_stats = Instant::now();
        }

        if args.busy_wait {
            std::hint::spin_loop();
        } else {
            std::thread::sleep(Duration::from_micros(100));
        }
    }

    let elapsed = start.elapsed();
    println!("\n--- Summary ---");
    for recv in &receivers {
        recv.stats.print_summary(recv.def.name, elapsed);
    }
    Ok(())
}

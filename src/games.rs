use std::collections::HashMap;
use std::io;

pub struct GameDef {
    pub id: &'static str,
    pub name: &'static str,
    pub family: &'static str,
    pub default_port: u16,
    pub process_names: &'static [&'static str],
    pub console: bool,
    pub notes: &'static str,
}

pub struct PortGroup {
    pub port: u16,
    pub display_name: String,
    pub id: String,
    pub process_names: Vec<&'static str>,
    pub console: bool,
}

pub static GAMES: &[GameDef] = &[
    // ── Forza (port 5300) ─────────────────────────────────────────────────────
    GameDef {
        id: "forza-fm7",
        name: "Forza Motorsport 7",
        family: "Forza",
        default_port: 5300,
        process_names: &["ForzaMotorsport7.exe"],
        console: false,
        notes: "Settings > HUD and Gameplay > Data Out > enable, Dash format.",
    },
    GameDef {
        id: "forza-fh4",
        name: "Forza Horizon 4",
        family: "Forza",
        default_port: 5300,
        process_names: &["ForzaHorizon4.exe"],
        console: false,
        notes: "Settings > HUD and Gameplay > Data Out > enable, Dash format.",
    },
    GameDef {
        id: "forza-fh5",
        name: "Forza Horizon 5",
        family: "Forza",
        default_port: 5300,
        process_names: &["ForzaHorizon5.exe"],
        console: false,
        notes: "Settings > HUD and Gameplay > Data Out > enable, Dash format.",
    },
    // ── Forza Motorsport 2023 (port 9876) ─────────────────────────────────────
    GameDef {
        id: "forza-fm",
        name: "Forza Motorsport (2023)",
        family: "Forza",
        default_port: 9876,
        process_names: &["ForzaMotorsport.exe"],
        console: false,
        notes: "Settings > HUD and Gameplay > Data Out > enable.",
    },
    // ── Project CARS 2 API (port 5606) ────────────────────────────────────────
    GameDef {
        id: "pcars2",
        name: "Project Cars 2",
        family: "Project CARS 2",
        default_port: 5606,
        process_names: &["pCARS2AVX.exe", "pCARS2.exe"],
        console: false,
        notes: "Enable UDP Frequency > 0 in game settings.",
    },
    GameDef {
        id: "ams2",
        name: "Automobilista 2",
        family: "Project CARS 2",
        default_port: 5606,
        process_names: &["AMS2AVX.exe", "AMS2.exe"],
        console: false,
        notes: "Enable UDP Frequency > 0 in game settings.",
    },
    GameDef {
        id: "kartkraft",
        name: "KartKraft",
        family: "Project CARS 2",
        default_port: 5606,
        process_names: &["KartKraft.exe"],
        console: false,
        notes: "Telemetry sent automatically.",
    },
    // ── BeamNG.drive (ports 9999 and 63392) ───────────────────────────────────
    GameDef {
        id: "beamng-sh",
        name: "BeamNG.drive (SimHub Mod)",
        family: "BeamNG",
        default_port: 9999,
        process_names: &["BeamNG.drive.exe"],
        console: false,
        notes: "Requires the SimHub telemetry mod installed in BeamNG.",
    },
    GameDef {
        id: "beamng-outgauge",
        name: "BeamNG.drive (OutGauge)",
        family: "BeamNG",
        default_port: 63392,
        process_names: &["BeamNG.drive.exe"],
        console: false,
        notes: "Options > Other > OutGauge > enable, IP 127.0.0.1, port 63392.",
    },
    // ── Codemasters / EA Sports (port 20777) ──────────────────────────────────
    GameDef {
        id: "f1-25",
        name: "F1 25",
        family: "Codemasters",
        default_port: 20777,
        process_names: &["F1_25.exe"],
        console: false,
        notes: "Game Options > Settings > Telemetry Settings > UDP On, port 20777.",
    },
    GameDef {
        id: "f1-24",
        name: "F1 24",
        family: "Codemasters",
        default_port: 20777,
        process_names: &["F1_24.exe"],
        console: false,
        notes: "Game Options > Settings > Telemetry Settings > UDP On, port 20777.",
    },
    GameDef {
        id: "f1-23",
        name: "F1 23",
        family: "Codemasters",
        default_port: 20777,
        process_names: &["F1_23.exe"],
        console: false,
        notes: "Game Options > Settings > Telemetry Settings > UDP On, port 20777.",
    },
    GameDef {
        id: "f1-22",
        name: "F1 22",
        family: "Codemasters",
        default_port: 20777,
        process_names: &["F1_22.exe"],
        console: false,
        notes: "Game Options > Settings > Telemetry Settings > UDP On, port 20777.",
    },
    GameDef {
        id: "f1-21",
        name: "F1 21",
        family: "Codemasters",
        default_port: 20777,
        process_names: &["F1_21.exe"],
        console: false,
        notes: "Game Options > Settings > Telemetry Settings > UDP On, port 20777.",
    },
    GameDef {
        id: "f1-20",
        name: "F1 2020",
        family: "Codemasters",
        default_port: 20777,
        process_names: &["F1_2020.exe"],
        console: false,
        notes: "Game Options > Settings > Telemetry Settings > UDP On, port 20777.",
    },
    GameDef {
        id: "f1-19",
        name: "F1 2019",
        family: "Codemasters",
        default_port: 20777,
        process_names: &["F1_2019.exe"],
        console: false,
        notes: "Game Options > Settings > Telemetry Settings > UDP On, port 20777.",
    },
    GameDef {
        id: "f1-18",
        name: "F1 2018",
        family: "Codemasters",
        default_port: 20777,
        process_names: &["F1_2018.exe"],
        console: false,
        notes: "Game Options > Settings > Telemetry Settings > UDP On, port 20777.",
    },
    GameDef {
        id: "dirt-rally2",
        name: "DiRT Rally 2.0",
        family: "Codemasters",
        default_port: 20777,
        process_names: &["dirtrally2.exe"],
        console: false,
        notes: "Hardware Settings > enable UDP, port 20777.",
    },
    GameDef {
        id: "dirt4",
        name: "DiRT 4",
        family: "Codemasters",
        default_port: 20777,
        process_names: &["dirt4.exe"],
        console: false,
        notes: "Hardware Settings > enable UDP, port 20777.",
    },
    GameDef {
        id: "dirt5",
        name: "DiRT 5",
        family: "Codemasters",
        default_port: 20777,
        process_names: &["DIRT5.exe"],
        console: false,
        notes: "Hardware Settings > enable UDP, port 20777.",
    },
    GameDef {
        id: "wrc-23",
        name: "WRC 2023",
        family: "Codemasters",
        default_port: 20777,
        process_names: &["WRC.exe"],
        console: false,
        notes: "Settings > Telemetry > enable UDP, port 20777.",
    },
    GameDef {
        id: "wrc-24",
        name: "WRC 2024",
        family: "Codemasters",
        default_port: 20777,
        process_names: &["WRC24.exe", "WRC.exe"],
        console: false,
        notes: "Settings > Telemetry > enable UDP, port 20777.",
    },
    // ── Wreckfest 2 (port 23123) ──────────────────────────────────────────────
    GameDef {
        id: "wreckfest2",
        name: "Wreckfest 2",
        family: "Wreckfest",
        default_port: 23123,
        process_names: &[
            "Wreckfest2.exe",
            "Wreckfest2_BE.exe",             // BattlEye anti-cheat launcher
            "Wreckfest2_EAC.exe",            // EasyAntiCheat launcher
            "Wreckfest2-Win64-Shipping.exe", // Unreal Engine shipping binary
        ],
        console: false,
        notes: "Requires config.json in the save folder: \
                %USERPROFILE%\\Documents\\My Games\\Wreckfest 2\\<ProfileID>\\savegame\\telemetry\\config.json \
                with content: {\"udp\":[{\"enabled\":1,\"ip\":\"127.0.0.1\",\"port\":\"23123\"}]}. \
                The telemetry folder and config.json must be created manually.",
    },
    // ── Gran Turismo (port 33740) ─────────────────────────────────────────────
    // Console titles — PS4/PS5 only; no PC process to detect.
    GameDef {
        id: "gt7",
        name: "Gran Turismo 7",
        family: "Gran Turismo",
        default_port: 33740,
        process_names: &[],
        console: true,
        notes: "Settings > Application > Machine Communication > Telemetry Port 33740.",
    },
    GameDef {
        id: "gt-sport",
        name: "Gran Turismo Sport",
        family: "Gran Turismo",
        default_port: 33740,
        process_names: &[],
        console: true,
        notes: "Settings > enable Telemetry (UDP), port 33740.",
    },
    // ── SCS Software / Giants (port 25555) ────────────────────────────────────
    GameDef {
        id: "ets2",
        name: "Euro Truck Simulator 2",
        family: "SCS",
        default_port: 25555,
        process_names: &["eurotrucks2.exe"],
        console: false,
        notes: "Install the SCS Telemetry plugin for SimHub.",
    },
    GameDef {
        id: "ats",
        name: "American Truck Simulator",
        family: "SCS",
        default_port: 25555,
        process_names: &["amtrucks.exe"],
        console: false,
        notes: "Install the SCS Telemetry plugin for SimHub.",
    },
    GameDef {
        id: "fs22",
        name: "Farming Simulator 22",
        family: "Giants",
        default_port: 25555,
        process_names: &["FarmingSimulator2022Game.exe"],
        console: false,
        notes: "Install the SimHub telemetry mod for FS22.",
    },
    GameDef {
        id: "fs25",
        name: "Farming Simulator 25",
        family: "Giants",
        default_port: 25555,
        process_names: &["FarmingSimulator2025.exe"],
        console: false,
        notes: "Install the SimHub telemetry mod for FS25.",
    },
    // ── Piboso / Live for Speed (port 30000) ──────────────────────────────────
    GameDef {
        id: "gpbikes",
        name: "GP Bikes",
        family: "Piboso",
        default_port: 30000,
        process_names: &["GPBikes.exe"],
        console: false,
        notes: "Telemetry sent automatically.",
    },
    GameDef {
        id: "mxbikes",
        name: "MX Bikes",
        family: "Piboso",
        default_port: 30000,
        process_names: &["MXBikes.exe"],
        console: false,
        notes: "Telemetry sent automatically.",
    },
    GameDef {
        id: "krp",
        name: "Kart Racing Pro",
        family: "Piboso",
        default_port: 30000,
        process_names: &["KartRacingPro.exe"],
        console: false,
        notes: "Telemetry sent automatically.",
    },
    GameDef {
        id: "lfs",
        name: "Live for Speed",
        family: "LFS",
        default_port: 30000,
        process_names: &["LFS.exe"],
        console: false,
        notes: "Options > Output > OutSim > enable, port 30000.",
    },
    // ── DCS World (port 34380) ────────────────────────────────────────────────
    GameDef {
        id: "dcs",
        name: "DCS World",
        family: "DCS",
        default_port: 34380,
        process_names: &["DCS.exe"],
        console: false,
        notes: "Requires a DCS export script — see SimHub DCS setup guide.",
    },
    // ── X-Plane (port 49003) ─────────────────────────────────────────────────
    GameDef {
        id: "xplane",
        name: "X-Plane 11/12",
        family: "X-Plane",
        default_port: 49003,
        process_names: &["X-Plane.exe"],
        console: false,
        notes: "Settings > Data Output > enable Network via UDP, port 49003.",
    },
    // ── NoLimits 2 (port 15151) ───────────────────────────────────────────────
    GameDef {
        id: "nolimits2",
        name: "NoLimits 2",
        family: "NoLimits2",
        default_port: 15151,
        process_names: &["NoLimits2.exe"],
        console: false,
        notes: "Telemetry sent automatically when a coaster is running.",
    },
];

fn port_dedup(defs: Vec<&'static GameDef>) -> Vec<PortGroup> {
    let mut order: Vec<u16> = Vec::new();
    let mut by_port: HashMap<u16, Vec<&'static GameDef>> = HashMap::new();
    for def in defs {
        let entry = by_port.entry(def.default_port).or_insert_with(|| {
            order.push(def.default_port);
            Vec::new()
        });
        entry.push(def);
    }
    order
        .into_iter()
        .map(|port| {
            let group_defs = &by_port[&port];
            let display_name = if group_defs.len() == 1 {
                group_defs[0].name.to_string()
            } else {
                let first_family = group_defs[0].family;
                if group_defs.iter().all(|d| d.family == first_family) {
                    format!("{first_family} \u{2014} port {port}")
                } else {
                    format!("port {port}")
                }
            };
            let id = if group_defs.len() == 1 {
                group_defs[0].id.to_string()
            } else {
                format!("port-{port}")
            };
            let mut process_names: Vec<&'static str> = group_defs
                .iter()
                .flat_map(|d| d.process_names.iter().copied())
                .collect();
            process_names.dedup();
            let console = group_defs.iter().all(|d| d.console);
            PortGroup {
                port,
                display_name,
                id,
                process_names,
                console,
            }
        })
        .collect()
}

pub fn select_games(games: &Option<Vec<String>>, all: bool) -> io::Result<Vec<PortGroup>> {
    let defs: Vec<&'static GameDef> = if all || games.as_ref().is_none_or(|v| v.is_empty()) {
        GAMES.iter().collect()
    } else {
        let ids = games.as_ref().unwrap();
        let mut selected = Vec::new();
        for id in ids {
            match GAMES.iter().find(|g| g.id == *id) {
                Some(def) => selected.push(def),
                None => {
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidInput,
                        format!(
                            "unknown game id '{id}' — run `sim-relay list` to see available games"
                        ),
                    ))
                }
            }
        }
        selected
    };
    Ok(port_dedup(defs))
}

pub fn print_list() {
    // Collect families in order of first appearance.
    let mut family_order: Vec<&'static str> = Vec::new();
    let mut by_family: HashMap<&'static str, Vec<&'static GameDef>> = HashMap::new();
    for game in GAMES {
        let entry = by_family.entry(game.family).or_insert_with(|| {
            family_order.push(game.family);
            Vec::new()
        });
        entry.push(game);
    }

    for family in &family_order {
        let games = &by_family[family];
        // Collect distinct ports for the header.
        let mut ports: Vec<u16> = Vec::new();
        for g in games.iter() {
            if !ports.contains(&g.default_port) {
                ports.push(g.default_port);
            }
        }
        let port_str = ports
            .iter()
            .map(|p| p.to_string())
            .collect::<Vec<_>>()
            .join(", ");
        println!("\n{family} (port {port_str})");
        for game in games.iter() {
            let console_note = if game.console {
                "  [console — use --include-console in auto mode]"
            } else {
                ""
            };
            println!(
                "  {:<18} {:<35} port {:>5}  {}{}",
                game.id, game.name, game.default_port, game.notes, console_note
            );
        }
    }

    println!("\nNOT SUPPORTED:");
    println!(
        "  assetto-corsa-evo  — uses shared memory (not UDP). \
         Planned: shared-memory forwarding like iracing-teleport."
    );
    println!("  assetto-corsa      — stateful handshake UDP; cannot be transparently forwarded.");
    println!("  assetto-corsa-rally — likely shared memory (research inconclusive).");
    println!(
        "  msfs2024           — uses SimConnect SDK (not UDP). \
         Requires a dedicated SimConnect relay tool."
    );
}

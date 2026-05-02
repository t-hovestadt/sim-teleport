use std::io;

pub struct GameDef {
    pub id: &'static str,
    pub name: &'static str,
    pub default_port: u16,
    pub detection: Detection,
    pub notes: &'static str,
}

pub enum Detection {
    /// Check for a running process by name (case-insensitive, any of the listed names).
    Process(&'static [&'static str]),
    /// Check for UDP traffic arriving on the listen port (passive).
    #[allow(dead_code)]
    UdpActivity,
}

pub static GAMES: &[GameDef] = &[
    GameDef {
        id: "pcars2",
        name: "Project Cars 2 / Automobilista 2",
        default_port: 5606,
        detection: Detection::Process(&["AMS2AVX.exe", "AMS2.exe", "pCARS2AVX.exe", "pCARS2.exe"]),
        notes: "Enable \"UDP Frequency\" > 0 in game settings.",
    },
    GameDef {
        id: "wreckfest2",
        name: "Wreckfest 2",
        default_port: 23123,
        detection: Detection::Process(&["Wreckfest2.exe"]),
        notes: "Telemetry is sent automatically — no in-game config needed.",
    },
    GameDef {
        id: "beamng-outgauge",
        name: "BeamNG.drive (OutGauge)",
        default_port: 63392,
        detection: Detection::Process(&["BeamNG.drive.exe"]),
        notes: "Options > Other > OutGauge > enable, set IP 127.0.0.1, port 63392.",
    },
    GameDef {
        id: "beamng-sh",
        name: "BeamNG.drive (SimHub Mod)",
        default_port: 9999,
        detection: Detection::Process(&["BeamNG.drive.exe"]),
        notes: "Requires the SimHub telemetry mod installed in BeamNG.",
    },
];

pub fn find_game(id: &str) -> Option<&'static GameDef> {
    GAMES.iter().find(|g| g.id == id)
}

pub fn select_games(games: &Option<Vec<String>>, all: bool) -> io::Result<Vec<&'static GameDef>> {
    if all || games.as_ref().is_none_or(|v| v.is_empty()) {
        return Ok(GAMES.iter().collect());
    }
    let ids = games.as_ref().unwrap();
    let mut selected = Vec::new();
    for id in ids {
        match find_game(id) {
            Some(def) => selected.push(def),
            None => {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    format!("unknown game id '{id}' — run `sim-relay list` to see available games"),
                ));
            }
        }
    }
    Ok(selected)
}

pub fn print_list() {
    println!("{:<20} {:>5}  SETUP NOTES", "ID", "PORT");
    println!("{}", "-".repeat(72));
    for game in GAMES {
        println!("{:<20} {:>5}  {}", game.id, game.default_port, game.notes);
        println!("  ({})", game.name);
        println!();
    }
    println!("NOT SUPPORTED:");
    println!(
        "  assetto-corsa-evo  — uses shared memory (not UDP). \
         Planned: shared-memory forwarding like iracing-teleport."
    );
    println!(
        "  assetto-corsa      — stateful handshake UDP; cannot be transparently forwarded. \
         Use shared-memory mirroring instead."
    );
    println!("  assetto-corsa-rally — likely shared memory (research inconclusive).");
    println!(
        "  msfs2024           — uses SimConnect SDK (not UDP). \
         Requires a dedicated SimConnect relay tool."
    );
}

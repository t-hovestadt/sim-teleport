# sim-relay

Forward sim racing game UDP telemetry from your gaming PC to a remote PC running
SimHub. The game sends packets to `localhost`, sim-relay intercepts them and
re-transmits the raw bytes to the target machine — SimHub sees them exactly as if
the game were running locally.

```
  Gaming PC (source)                           SimHub PC (target)

  ┌──────────────────┐                     ┌──────────────────┐
  │  Game             │                     │  SimHub           │
  │  sends UDP to     │                     │  listens on       │
  │  localhost:5606   │                     │  :5606            │
  └────────┬─────────┘                     └────────▲─────────┘
           │                                        │
  ┌────────▼─────────┐   UDP (raw bytes)   ┌────────┴─────────┐
  │  sim-relay        │ ──────────────────► │  sim-relay        │
  │  source           │                     │  target           │
  │  binds :5606      │                     │  listens :15606   │
  └──────────────────┘                     │  forwards :5606   │
                                            └──────────────────┘
```

No compression, no framing, no state — raw UDP bytes in, raw UDP bytes out.

**For all games in one app:** [sim-bridge](https://github.com/t-hovestadt/sim-bridge)
bundles Sim Relay with iRacing Teleport and AC Teleport. One binary,
automatic game detection.

**Companion projects:**
- [iracing-teleport](https://github.com/t-hovestadt/iracing-teleport) — iRacing (shared memory, not UDP)
- [ac-teleport](https://github.com/t-hovestadt/ac-teleport) — Assetto Corsa / ACE (shared memory)
- [sim-bridge](https://github.com/t-hovestadt/sim-bridge) — unified single-binary launcher for all three

---

## Download

Pre-built Windows x64 binaries are on the [Releases](../../releases/latest) page.

| File | Machine |
|------|---------|
| `source.exe` | Gaming PC |
| `target.exe` | SimHub PC |
| `sim-relay.exe` | Either — combined CLI (`sim-relay source` / `sim-relay target`) |

---

## Windows SmartScreen

On first run, Windows may show "Windows protected your PC." This is normal for unsigned open-source software.

To unblock: right-click the `.exe` → **Properties** → check **Unblock** at the bottom of the General tab → **OK**.

Or click **More info** on the SmartScreen dialog, then **Run anyway**.

---

## Quick start

**Gaming PC:**
```
source.exe --target 192.168.50.2
```

Scans for running game processes every 5 seconds. When a supported game is
detected it binds the port and starts forwarding. When the game exits it drains
for 15 seconds then releases the port. No other flags needed — start once and
leave it running.

**SimHub PC (optional):**
```
target.exe
```

SimHub can often receive forwarded UDP directly from the source. `target.exe` is
needed when you want sim-relay to forward to a non-default port or address.

**List supported games:**
```
sim-relay.exe list
```

---

## Port offset and conflict avoidance

When using sim-relay standalone, source sends directly to `target:(game_port)`.
SimHub on the target listens on the same port — this usually works because SimHub
binds first and receives each unicast packet.

When using sim-bridge (the unified launcher), a `--port-offset N` (default: 10000)
is applied to both sides:
- Source sends to `target:(game_port + N)` — e.g., Wreckfest 2 → port 33123
- Target listens on `game_port + N` and forwards to `127.0.0.1:game_port`
- SimHub reads from `game_port` as usual

This avoids any socket-sharing dependency between sim-relay target and SimHub.
No start-order sensitivity; no `SO_REUSEADDR` race.

**BeamNG OutGauge (port 63392)** overflows at offset 10000: 63392 + 10000 = 73392
exceeds the valid UDP port range (65535). If you need BeamNG OutGauge, set
`apps.relay_port_offset` to ≤ 2143 in `sim-bridge.toml`.

---

## Supported games

Run `sim-relay list` for the full table with per-game setup notes.

### Forza

| ID | Game | Port | Setup |
|----|------|------|-------|
| `forza-fm7` | Forza Motorsport 7 | 5300 | Settings → HUD and Gameplay → Data Out → enable, **Dash format** |
| `forza-fh4` | Forza Horizon 4 | 5300 | Same |
| `forza-fh5` | Forza Horizon 5 | 5300 | Same |
| `forza-fm` | Forza Motorsport (2023) | 9876 | Same |

Detection: `ForzaMotorsport7.exe`, `ForzaHorizon4.exe`, `ForzaHorizon5.exe`, `ForzaMotorsport.exe`

### Project CARS 2 API (port 5606)

| ID | Game | Detection |
|----|------|-----------|
| `pcars2` | Project Cars 2 | `pCARS2AVX.exe`, `pCARS2.exe` |
| `ams2` | Automobilista 2 | `AMS2AVX.exe`, `AMS2.exe` |
| `kartkraft` | KartKraft | `KartKraft.exe` |

Enable **UDP Frequency > 0** in game settings.

### BeamNG.drive

| ID | Game | Port | Setup |
|----|------|------|-------|
| `beamng-sh` | BeamNG.drive (SimHub Mod) | 9999 | Install SimHub telemetry mod |
| `beamng-outgauge` | BeamNG.drive (OutGauge) | 63392 | Options → Other → OutGauge → enable, port `63392`. **Note: port overflows at relay_port_offset 10000 — see above.** |

Detection: `BeamNG.drive.exe` (for both)

### Codemasters / EA Sports (port 20777)

| ID | Game |
|----|------|
| `f1-25` | F1 25 |
| `f1-24` | F1 24 |
| `f1-23` | F1 23 |
| `f1-22` | F1 22 |
| `f1-21` | F1 21 |
| `f1-20` | F1 2020 |
| `f1-19` | F1 2019 |
| `f1-18` | F1 2018 |
| `dirt-rally2` | DiRT Rally 2.0 |
| `dirt4` | DiRT 4 |
| `dirt5` | DiRT 5 |
| `wrc-23` | WRC 2023 |
| `wrc-24` | WRC 2024 |

Game Options → Settings → **Telemetry Settings** → UDP On, port 20777.
Detection: `F1_25.exe`, `F1_24.exe`, …, `dirtrally2.exe`, `DIRT5.exe`, `WRC.exe`, `WRC24.exe`

### Wreckfest 2 (port 23123)

**ID:** `wreckfest2`

Detection: `Wreckfest2.exe`, `Wreckfest2_BE.exe` (BattlEye), `Wreckfest2_EAC.exe`
(EasyAntiCheat), `Wreckfest2-Win64-Shipping.exe` (Unreal Engine shipping binary)

Wreckfest 2 requires a config file to enable UDP telemetry:

Path: `%USERPROFILE%\Documents\My Games\Wreckfest 2\<SteamID>\savegame\telemetry\config.json`

```json
{
  "udp": [
    {
      "enabled": 1,
      "ip": "127.0.0.1",
      "port": "23123"
    }
  ]
}
```

`<SteamID>` is the numbered folder inside `My Games\Wreckfest 2\`. Create the
`telemetry` folder if it doesn't exist. Restart the game after creating the file.

sim-bridge source creates this file automatically when it detects the save
directory exists (i.e., you have run Wreckfest 2 at least once). Look for:
```
[Wreckfest 2] Created telemetry config — restart game to activate
```

### Gran Turismo (port 33740 — console, no PC process)

| ID | Game |
|----|------|
| `gt7` | Gran Turismo 7 |
| `gt-sport` | Gran Turismo Sport |

Settings → enable UDP telemetry, port 33740. No PC process to detect —
pass `--include-console` to always bind port 33740, or use `--games gt7`.

### Truck and farm sims (port 25555)

| ID | Game | Detection |
|----|------|-----------|
| `ets2` | Euro Truck Simulator 2 | `eurotrucks2.exe` |
| `ats` | American Truck Simulator | `amtrucks.exe` |
| `fs22` | Farming Simulator 22 | `FarmingSimulator2022Game.exe` |
| `fs25` | Farming Simulator 25 | `FarmingSimulator2025.exe` |

ETS2/ATS: install the [SCS Telemetry plugin](https://github.com/RenCloud/scs-sdk-plugin).
FS22/FS25: install the SimHub telemetry mod.

### Piboso / Live for Speed (port 30000)

| ID | Game | Detection |
|----|------|-----------|
| `gpbikes` | GP Bikes | `GPBikes.exe` |
| `mxbikes` | MX Bikes | `MXBikes.exe` |
| `krp` | Kart Racing Pro | `KartRacingPro.exe` |
| `lfs` | Live for Speed | `LFS.exe` |

Piboso titles send automatically. LFS: Options → Output → OutSim → enable, port 30000.

### Other

| ID | Game | Port | Detection | Setup |
|----|------|------|-----------|-------|
| `dcs` | DCS World | 34380 | `DCS.exe` | Requires DCS export script |
| `xplane` | X-Plane 11/12 | 49003 | `X-Plane.exe` | Settings → Data Output → Network via UDP, port 49003 |
| `nolimits2` | NoLimits 2 | 15151 | `NoLimits2.exe` | Telemetry sent automatically |

### Not supported

| Game | Reason |
|------|--------|
| **Assetto Corsa EVO** | Shared memory (not UDP). Use [ac-teleport](https://github.com/t-hovestadt/ac-teleport). |
| **Assetto Corsa (original)** | Stateful handshake UDP — requires subscribe/response protocol, not transparently proxyable. |
| **Microsoft Flight Simulator 2024** | SimConnect SDK (named pipe / TCP), not UDP. |

---

## Options

| Flag | source | target | Default | Description |
|------|:------:|:------:|---------|-------------|
| `--target <IP>` | ✓ | | — | **Required for source.** Target PC IP address. |
| `--games <id,...>` | ✓ | ✓ | *(auto)* | Comma-separated game IDs to forward/receive. |
| `--all` | ✓ | ✓ | off | Bind all ports at startup; skip process detection. Alias: `--force-bind`. |
| `--force-bind` | ✓ | | off | Alias for `--all`. |
| `--scan-interval <SECS>` | ✓ | | `5` | How often to scan for game processes. |
| `--grace-period <SECS>` | ✓ | | `15` | How long to keep forwarding after game exits. |
| `--include-console` | ✓ | | off | Include GT7/GT Sport in auto-detect (no PC process — always bind). |
| `--local-forward` | ✓ | | off | Also forward to `localhost:<port+1000>` for a local SimHub instance. |
| `--bind <IP>` | ✓ | | `0.0.0.0` | Bind address for listen sockets. |
| `--high-priority` | ✓ | ✓ | off | `HIGH_PRIORITY_CLASS` for lower scheduling jitter. |
| `--source <IP>` | | ✓ | — | Source PC IP (informational only, for logging). |
| `--forward-to <IP:PORT>` | | ✓ | `127.0.0.1:<game_port>` | Override forwarding destination. |
| `--busy-wait` | | ✓ | off | Spin on recv instead of sleeping (lower latency, higher CPU). |
| `--port-offset <N>` | ✓ | ✓ | `0` | Add N to send/listen port. Source sends to `target:(game_port+N)`; target listens on `game_port+N` and forwards to `127.0.0.1:game_port`. Used by sim-bridge (offset 10000) to avoid binding conflicts. |

---

## Auto-detection (default)

Auto-detection is the default. No flags needed beyond `--target`:

```
source.exe --target 192.168.50.2
```

Every 5 seconds (configurable with `--scan-interval`), sim-relay takes a single
Windows process snapshot and checks which games are running. When detected:
```
[F1 25] detected — binding port 20777
```

When the game exits, forwarding continues for 15 seconds (grace period) then:
```
[F1 25] drain expired — unbound port 20777
```

State machine per game:

```
Idle ──(detected)──► Active ──(exits)──► Draining ──(grace expires)──► Idle
                                              │
                                        (game returns)
                                              │
                                              ▼
                                            Active
```

**One active game at a time** in auto-detect mode. If two games are running
simultaneously, the one appearing first in the game registry wins. The second
activates only after the first finishes its drain and returns to Idle.

**Console games** (GT7, Gran Turismo Sport): no PC process to detect. Skip by
default; pass `--include-console` to always bind port 33740.

**Skip detection entirely:** `--all` binds all ports at startup:
```
source.exe --target 192.168.50.2 --all
```

Process detection is Windows-only. On other platforms auto-detect prints a
warning; use `--all`.

---

## How it works

**Source** (auto-detect mode): scans for game processes every 5 seconds. When a
game is detected, binds that port with `SO_REUSEADDR` and starts a non-blocking
drain loop reading all queued packets and re-transmitting them raw to the target.

When active, the loop sleeps 100 µs between iterations. When all relays are idle,
it sleeps 100 ms — near-zero idle CPU.

**Target** (optional): listens on `game_port + offset` and forwards raw bytes to
`127.0.0.1:game_port`. A separate forwarding socket prevents loopback self-receive.

**No-packets warning**: if a relay is active (game detected) but no packets arrive
for 15 seconds, a warning is logged. This usually means the game's telemetry setting
is disabled or set to the wrong IP/port.

Stats line every 5 seconds (active relays only):
```
[Project Cars 2 / Automobilista 2]  60.1 pkt/s   72.5 KB/s   avg 1205 b/pkt   3 µs fwd
```
The `3 µs fwd` figure is the socket send latency only (not including network transit).

---

## Compatible apps

Any app that reads the expected UDP port works on the target machine — packets
arrive on the same port the game normally sends to.

- [SimHub](https://www.simhubdash.com) — dashboards, overlays, haptics, LED control
- [RaceLab](https://racelab.app) — modern overlay suite
- [iOverlay](https://ioverlay.app) — standings and timing overlays
- [Z1 Dashboard](https://www.z1racetech.com) — live telemetry and lap analysis

---

## Port conflict notes

sim-relay source binds game ports with `SO_REUSEADDR`. On Windows, when two sockets
are bound to the same UDP port, only one receives each unicast packet (last-bound
wins). This means sim-relay source and a local SimHub cannot simultaneously receive
the same game packets.

**Option A — `--local-forward`**: sim-relay source also forwards to
`localhost:<port+1000>`. Configure local SimHub to listen on that offset port.
```
source.exe --target 192.168.50.2 --local-forward
```

**Option B — `--port-offset N` (recommended with sim-bridge)**: no socket sharing.
Source sends to `target:(port+N)`, target forwards to `localhost:port`. SimHub
reads from `port` as usual. Start order doesn't matter.

**Option C — SimHub only on the target PC.** Cleanest setup.

---

## Direct Ethernet setup

**1. Assign static IPs**

| PC | IP | Subnet |
|----|-----|--------|
| Gaming PC | `192.168.50.1` | `255.255.255.0` |
| SimHub PC | `192.168.50.2` | `255.255.255.0` |

In Windows: *Network & Internet → Change adapter options → right-click adapter →
Properties → IPv4 → Use the following IP address*. Leave gateway and DNS blank.

**2. Firewall rules (SimHub PC)** — run as Administrator:

```powershell
New-NetFirewallRule -DisplayName "Sim Relay" -Direction Inbound -Protocol UDP `
    -LocalPort 5300,5606,9876,9999,15151,20777,23123,25555,30000,33740,34380,49003,63392 `
    -Action Allow
```

When using sim-bridge with the default port offset of 10000, add 10000 to each port
(e.g., 33123 for Wreckfest 2, not 23123). Run `sim-bridge firewall` for the
exact combined list.

**3. NIC settings (both PCs)**

| Setting | Value |
|---------|-------|
| Energy Efficient Ethernet | Disabled |
| Interrupt Moderation / Interrupt Throttle Rate | Disabled |
| Wake on Magic Packet | Disabled |
| Wake on Pattern Match | Disabled |
| Auto MDI/MDIX | Auto |
| Speed & Duplex | 1.0 Gbps Full Duplex |

Power Management: uncheck "Allow the computer to turn off this device" and
"Allow this device to wake the computer."

**4. Bat files**

`start-source.bat` on the **gaming PC**:
```batch
@echo off
cd /d "%~dp0"
source.exe --target 192.168.50.2
pause
```

`start-target.bat` on the **SimHub PC**:
```batch
@echo off
cd /d "%~dp0"
target.exe
pause
```

**Troubleshooting**

*Adapter shows Disconnected:* Full Shut down (not Restart), wait 30–60 s, power on.
Disable Wake-on-LAN in NIC settings and BIOS.

*Link won't establish:* Set Speed & Duplex to 1.0 Gbps Full Duplex; confirm Auto
MDI/MDIX is Auto.

*Can't set static IP:* Plug cable in first. To reset:
`Remove-NetIPAddress -InterfaceIndex <N> -Confirm:$false`.

---

## Running alongside local SimHub (source PC)

If SimHub is also on the gaming PC, see [Port conflict notes](#port-conflict-notes).
The cleanest approach is `--port-offset` with sim-bridge, or run SimHub only
on the target PC.

---

## Building from source

Requires [Rust](https://rustup.rs) (stable).

```
git clone https://github.com/t-hovestadt/sim-relay
cd sim-relay
cargo build --release
```

Cross-compile for Windows from macOS:

```
rustup target add x86_64-pc-windows-gnu
brew install mingw-w64
cargo build --release --target x86_64-pc-windows-gnu
```

If your working directory path contains spaces, set `CARGO_TARGET_DIR` to a
path without spaces — the `mingw-w64` linker doesn't handle quoted paths.

---

## Library API

sim-relay is a library crate (`sim-relay = { path = "…" }`). The public API
surface used by sim-bridge:

```rust
// Target args
pub struct TargetArgs {
    pub source: Option<String>,           // Source PC IP (informational)
    pub games: Option<Vec<String>>,       // Game IDs to forward; None = all
    pub all: bool,                        // Bind all ports at startup
    pub forward_to: Option<String>,       // Override destination
    pub high_priority: bool,
    pub busy_wait: bool,
    pub on_game_active: Option<Arc<dyn Fn(&str, bool) + Send + Sync>>,
    pub port_offset: u16,                 // Add to listen port
}

pub mod target {
    pub fn run(args: TargetArgs, shutdown: Receiver<()>) -> anyhow::Result<()>;
}
```

`on_game_active(game_id, is_active)` fires when a game becomes active (first packet
received after a period of silence) or inactive (no packets for a timeout). sim-bridge
uses this to call `SimHubWPF.exe -switchgame` with the correct SimHub code.

---

## License

MIT

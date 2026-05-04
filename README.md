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
  │  sim-relay        │ ──────────────────► │  (SimHub direct) │
  │  source           │                     │  or sim-relay     │
  │  binds :5606      │                     │  target           │
  └──────────────────┘                     └──────────────────┘
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
| `source-vX.X.X-windows-x86_64.exe` | Gaming PC |
| `target-vX.X.X-windows-x86_64.exe` | SimHub PC |
| `sim-relay-vX.X.X-windows-x86_64.exe` | Either (combined CLI: `sim-relay source` / `sim-relay target`) |

Rename the downloaded files to `source.exe`, `target.exe`, and `sim-relay.exe` and place them in the same folder.

---

## Windows SmartScreen

On first run, Windows may show "Windows protected your PC." This is normal for unsigned open-source software.

To unblock: right-click the `.exe` → **Properties** → check **Unblock** at the bottom of the General tab → **OK**.

Or click **More info** on the SmartScreen dialog, then **Run anyway**.

---

## Quick Start

**Gaming PC:**
```
source.exe --target <SimHub-PC-IP>
```
Scans for running game processes every 5 seconds. When it detects a supported game it binds that port and starts forwarding. When the game exits it drains for 15 seconds then releases the port. No flags needed — start it once and leave it running.

**SimHub PC (optional — only needed if SimHub is on a non-default port):**
```
target.exe
```

**List supported games:**
```
sim-relay.exe list
```

Both `source.exe` and `target.exe` are also available as subcommands of the combined `sim-relay.exe`:
```
sim-relay.exe source --target <IP>
sim-relay.exe target
```

**Direct Ethernet (192.168.50.1 → 192.168.50.2):** See the [Direct Ethernet setup](#direct-ethernet-setup) section.

---

## Supported Games

`sim-relay list` prints the full table with per-game setup notes. Games sharing a UDP port are listened to on a single socket — one port, one socket, regardless of how many games use it.

**Forza** (port 5300 — FM7, FH4, FH5 · port 9876 — FM 2023)

| ID | Game | Port | Detection |
|----|------|------|-----------|
| `forza-fm7` | Forza Motorsport 7 | 5300 | `ForzaMotorsport7.exe` |
| `forza-fh4` | Forza Horizon 4 | 5300 | `ForzaHorizon4.exe` |
| `forza-fh5` | Forza Horizon 5 | 5300 | `ForzaHorizon5.exe` |
| `forza-fm` | Forza Motorsport (2023) | 9876 | `ForzaMotorsport.exe` |

Settings → HUD and Gameplay → Data Out → enable, **Dash format**.

**Project CARS 2 API** (port 5606)

| ID | Game | Detection |
|----|------|-----------|
| `pcars2` | Project Cars 2 | `pCARS2AVX.exe`, `pCARS2.exe` |
| `ams2` | Automobilista 2 | `AMS2AVX.exe`, `AMS2.exe` |
| `kartkraft` | KartKraft | `KartKraft.exe` |

Enable **UDP Frequency > 0** in game settings.

**BeamNG.drive** (ports 9999, 63392)

| ID | Game | Port | Setup |
|----|------|------|-------|
| `beamng-sh` | BeamNG.drive (SimHub Mod) | 9999 | Requires the SimHub telemetry mod |
| `beamng-outgauge` | BeamNG.drive (OutGauge) | 63392 | Options → Other → OutGauge → enable, port `63392` |

**Codemasters / EA Sports** (port 20777 — all titles)

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

Game Options → Settings → **Telemetry Settings** → UDP On, port 20777. (F1 games) or Hardware Settings → UDP (DiRT / WRC).

**Wreckfest 2** (port 23123) — `wreckfest2` — requires a config file (not automatic).

Create the folder and file manually:

Path: `%USERPROFILE%\Documents\My Games\Wreckfest 2\<ProfileID>\savegame\telemetry\config.json`

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

`<ProfileID>` is a Steam ID number — look for the numbered folder inside `My Games\Wreckfest 2\`. Create the `telemetry` folder if it doesn't exist. Restart the game after creating the file.

**Gran Turismo** (port 33740 — PS4/PS5 console, no PC process to detect)

| ID | Game |
|----|------|
| `gt7` | Gran Turismo 7 |
| `gt-sport` | Gran Turismo Sport |

Settings → enable UDP telemetry, port 33740. In auto-detect mode, pass `--include-console` to bind port 33740 (or use `--games gt7`).

**Truck / Farm Sims** (port 25555)

| ID | Game | Detection |
|----|------|-----------|
| `ets2` | Euro Truck Simulator 2 | `eurotrucks2.exe` |
| `ats` | American Truck Simulator | `amtrucks.exe` |
| `fs22` | Farming Simulator 22 | `FarmingSimulator2022Game.exe` |
| `fs25` | Farming Simulator 25 | `FarmingSimulator2025.exe` |

ETS2/ATS: install the [SCS Telemetry plugin](https://github.com/RenCloud/scs-sdk-plugin). FS22/FS25: install the SimHub telemetry mod.

**Piboso / Live for Speed** (port 30000)

| ID | Game | Detection |
|----|------|-----------|
| `gpbikes` | GP Bikes | `GPBikes.exe` |
| `mxbikes` | MX Bikes | `MXBikes.exe` |
| `krp` | Kart Racing Pro | `KartRacingPro.exe` |
| `lfs` | Live for Speed | `LFS.exe` |

Piboso titles send automatically. LFS: Options → Output → OutSim → enable, port 30000.

**DCS World** (port 34380) — `dcs` — requires a DCS export script.

**X-Plane 11/12** (port 49003) — `xplane` — Settings → Data Output → Network via UDP, port 49003.

**NoLimits 2** (port 15151) — `nolimits2` — telemetry sent automatically.

### Not Supported

| Game | Reason |
|------|--------|
| **Assetto Corsa EVO** | Uses shared memory (not UDP). Supported via [ac-teleport](https://github.com/t-hovestadt/ac-teleport) and [sim-bridge](https://github.com/t-hovestadt/sim-bridge). |
| **Assetto Corsa (original)** | Stateful handshake UDP — the game's UDP server requires a subscribe/response protocol that cannot be transparently proxied. |
| **Assetto Corsa Rally** | Likely shared memory (research inconclusive on UDP exposure). |
| **Microsoft Flight Simulator 2024** | Uses SimConnect SDK (named pipe / TCP), not UDP. Requires a dedicated SimConnect relay. |

---

## Options

| Flag | source | target | Default | Description |
|------|:------:|:------:|---------|-------------|
| `--target <IP>` | ✓ | | — | **Required for source.** Target PC IP address |
| `--games <id,...>` | ✓ | ✓ | (auto) | Comma-separated game IDs to forward/receive |
| `--all` | ✓ | ✓ | off | Bind all ports immediately, skip process detection |
| `--force-bind` | ✓ | | off | Alias for `--all` |
| `--scan-interval <SECS>` | ✓ | | `5` | How often to scan for game processes |
| `--grace-period <SECS>` | ✓ | | `15` | How long to keep forwarding after a game exits |
| `--include-console` | ✓ | | off | Include GT7/GT Sport in auto-detect (no PC process; always bind) |
| `--local-forward` | ✓ | | off | Also forward to `localhost:<port+1000>` for a local SimHub instance |
| `--bind <IP>` | ✓ | | `0.0.0.0` | Bind address for listen sockets |
| `--high-priority` | ✓ | ✓ | off | Raise process to `HIGH_PRIORITY_CLASS` for lower scheduling jitter |
| `--source <IP>` | | ✓ | — | Source PC IP (informational only) |
| `--forward-to <IP:PORT>` | | ✓ | `127.0.0.1:<game_port>` | Override forwarding destination |
| `--busy-wait` | | ✓ | off | Spin on recv instead of sleeping (lower latency, higher CPU) |
| `--port-offset <N>` | ✓ | ✓ | `0` | Add N to send/listen port. Source sends to `target:(game_port+N)`; target listens on `game_port+N` and forwards to `127.0.0.1:game_port`. Used by sim-bridge (offset 10000) to avoid binding conflicts with SimHub on the target PC. |

---

## How It Works

- **source** (auto-detect mode, default) scans for running game processes every 5 seconds and binds a port only when that game is running. With `--all` it binds all ports at startup. Packets are read in a non-blocking drain loop and re-transmitted raw to the target PC — SimHub receives the same bytes the game sent.
- **target** (optional) listens on all game ports and forwards to `127.0.0.1:<port>` so SimHub on the target PC receives packets on the expected port. A separate forwarding socket prevents loopback self-receive. Most users don't need `target.exe` — SimHub can receive UDP from source directly.
- **Drain loop**: both source and target drain all queued packets from each socket per iteration. When any relay is active the loop sleeps 100 µs; when all are idle it sleeps 100 ms (near-zero idle CPU).

Both tools print a stats line every 5 s and a summary on Ctrl-C:

```
[Project Cars 2 / Automobilista 2]  60.1 pkt/s   72.5 KB/s   avg 1205 b/pkt   3 µs fwd
```

Only active relays (with packets flowing) produce output — idle games are silent.
The `µs fwd` figure is the average time to forward each packet (socket send latency only).

---

## Compatible Apps

Any app that reads SimHub-compatible UDP telemetry works on the target machine — packets
arrive on the same port the game would normally send to.

- [SimHub](https://www.simhubdash.com) — dashboards, overlays, haptics, LED control
- [RaceLab](https://racelab.app) — modern overlay suite
- [iOverlay](https://ioverlay.app) — standings and timing overlays
- [Z1 Dashboard](https://www.z1racetech.com) — live telemetry display and lap analysis

---

## Building from Source

Requires [Rust](https://rustup.rs) (stable).

```
git clone https://github.com/t-hovestadt/sim-relay
cd sim-relay
cargo build --release
```

Binary is written to `target/release/sim-relay.exe` (Windows) or `target/release/sim-relay`.

**Cross-compiling for Windows from macOS:**

```
rustup target add x86_64-pc-windows-gnu
brew install mingw-w64
cargo build --release --target x86_64-pc-windows-gnu
```

> If your working directory path contains spaces, set `CARGO_TARGET_DIR` to a path without
> spaces — the `mingw-w64` linker doesn't handle quoted paths.

---

## Direct Ethernet Setup

A direct Ethernet cable between the two PCs (no router, no switch) gives the lowest
possible latency. You need:

- A network adapter on each PC (PCIe/M.2 cards work well; USB adapters also work)
- A Cat 5e or better Ethernet cable
- Static IP addresses (Windows won't auto-assign usable IPs on a direct link)

**1. Assign static IPs**

On each PC, set a static IP on the direct-link adapter:

| PC | IP | Subnet |
|----|-----|--------|
| Gaming PC | `192.168.50.1` | `255.255.255.0` |
| SimHub PC | `192.168.50.2` | `255.255.255.0` |

In Windows: *Network & Internet → Change adapter options → right-click adapter → Properties → IPv4 → Use the following IP address*. Leave gateway and DNS blank.

**2. Firewall rules (SimHub PC)**

Add an inbound UDP rule for the game ports on the **SimHub PC**:

```powershell
# Run as Administrator
New-NetFirewallRule -DisplayName "Sim Relay" -Direction Inbound -Protocol UDP `
    -LocalPort 5300,5606,9876,9999,15151,20777,23123,25555,30000,33740,34380,49003,63392 -Action Allow
```

Or via *Windows Defender Firewall → Advanced Settings → Inbound Rules → New Rule → Port → UDP → enter the ports above → Allow*.

**3. NIC settings (both PCs)**

In Device Manager → Network Adapters → right-click the direct-link adapter → Properties, apply these settings on **both** machines:

**Advanced tab:**

| Setting | Value |
|---------|-------|
| Energy Efficient Ethernet | Disabled |
| Interrupt Moderation / Interrupt Throttle Rate | Disabled |
| Wake on Magic Packet | Disabled |
| Wake on Pattern Match | Disabled |
| Auto MDI/MDIX | Auto |
| Speed & Duplex | 1.0 Gbps Full Duplex |

**Power Management tab:**
- Uncheck **"Allow the computer to turn off this device to save power"**
- Uncheck **"Allow this device to wake the computer"**

Setting names vary by NIC manufacturer — look for equivalents if the exact names differ.

**4. Bat files**

`start-source.bat` on the **gaming PC** — place it next to `source.exe`:

```batch
@echo off
cd /d "%~dp0"
source.exe --target 192.168.50.2
pause
```

`start-target.bat` on the **SimHub PC** — place it next to `target.exe`:

```batch
@echo off
cd /d "%~dp0"
target.exe
pause
```

Both bat files are included in the release download. Edit `start-source.bat` to replace `192.168.50.2` with your SimHub PC's IP address.

**Troubleshooting**

*Adapter shows Disconnected despite cable plugged in:* Wake-on-LAN or PCIe ASPM can leave
the NIC in a state a warm reboot doesn't clear. Do a full **Shut down** (not Restart), wait
30–60 seconds for capacitors to drain, then power on. Disable Wake-on-LAN in the NIC settings
above and in BIOS (look for "Wake on LAN" or "PCIe ASPM").

*Link won't establish between two NICs:* Some NIC brands fail auto-negotiation on a direct
connection. The Speed & Duplex setting above (1.0 Gbps Full Duplex) fixes this. Also confirm
**Auto MDI/MDIX** is set to Auto — if disabled, a straight-through cable won't link without a
crossover cable.

*Can't set static IP via PowerShell (`element not found`):* Plug the cable in first so the
adapter shows a link, then set the IP. If the error is `already exists`, the IP may already be
configured — check with `Get-NetIPAddress`. To reset: `Remove-NetIPAddress -InterfaceIndex <N> -Confirm:$false`.

---

## Running Alongside Local SimHub (Source PC)

If SimHub is also installed on the gaming PC, it normally binds the same UDP port as the game.
When sim-relay source binds that port with `SO_REUSEADDR`, only one of them receives each packet
(Windows delivers UDP unicast to the most recently bound socket).

**Option A — Use `--local-forward`:**

sim-relay source also forwards each packet to `localhost:<port+1000>`. Configure SimHub on the
source PC to listen on that offset port (e.g. pcars2 → 6606 instead of 5606).

```
source.exe --target 192.168.50.2 --local-forward
```

**Option B — Run SimHub only on the target PC.** Cleanest setup.

---

## Auto-Detection

Auto-detection is the **default** in v0.1.4. No flags needed:

```
source.exe --target 192.168.50.2
```

**How it works:**
- Every 5 seconds (configurable with `--scan-interval`) sim-relay takes a single Windows process snapshot and checks which games are running.
- When a game process is detected, the corresponding UDP port is bound and forwarding starts. You'll see `[F1 25] detected — binding port 20777`.
- When the game process exits, forwarding continues for a 15-second grace period (configurable with `--grace-period`) to flush any last packets. Then the port is released. You'll see `[F1 25] drain expired — unbound port 20777`.
- If a game restarts while the grace period is still running, the relay resumes immediately without waiting.

**State transitions:**

```
Idle ──(game detected)──► Active ──(game exits)──► Draining ──(grace expires)──► Idle
                                                        │
                                                  (game returns)
                                                        │
                                                        ▼
                                                      Active
```

**One active game at a time:** In auto-detect mode only one relay is active at a time. If two games are running simultaneously, the one that appears first in the game list takes priority. A second game activates only after the first finishes its grace period and returns to Idle.

**Console games (GT7, Gran Turismo Sport):** These run on a PS4/PS5 — there is no Windows process to detect. In auto-detect mode they are skipped by default. Pass `--include-console` to always bind port 33740.

**Skip detection entirely:** Use `--all` to bind all 13 ports at startup (the v0.1.3 behavior):
```
source.exe --target 192.168.50.2 --all
```

Process detection is Windows-only. On other platforms auto-detect mode prints a warning and you should use `--all`.

---

## Port Conflict Notes

sim-relay source binds game ports with `SO_REUSEADDR`. On Windows, when two sockets are bound to
the same UDP port, only one receives each unicast packet (generally last-bound wins). This means
sim-relay source and SimHub cannot simultaneously receive the same game packets — use
`--local-forward` if you need both.

On the target PC, sim-relay target and SimHub can share a port with `SO_REUSEADDR` if started in
the right order (start sim-relay target first, SimHub second). Alternatively use `--forward-to`
to point sim-relay target at a different port and configure SimHub accordingly.

The cleanest solution is `--port-offset N` (applied to both source and target): source sends to
`target:(game_port+N)` and target listens on `game_port+N`, forwarding to `127.0.0.1:game_port`
where SimHub reads. No socket sharing, no start-order dependency. sim-bridge uses offset 10000 by
default. Note: BeamNG OutGauge (port 63392) overflows at offset 10000 — use offset ≤ 2143 if you
need that game.

---

## License

MIT

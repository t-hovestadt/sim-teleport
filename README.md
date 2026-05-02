# sim-relay

Forward sim racing game UDP telemetry from your gaming PC to a remote PC running
SimHub. The game sends packets to `localhost`, sim-relay intercepts them and
re-transmits the raw bytes to the target machine — SimHub sees them exactly as if
the game were running locally.

**Companion to [iracing-teleport](../teleport/)**, which handles iRacing via shared
memory. sim-relay covers games that broadcast UDP telemetry natively.

## Supported Games

| ID | Game | Port | Detection | Setup |
|----|------|------|-----------|-------|
| `pcars2` | Project Cars 2 / Automobilista 2 | 5606 | `AMS2AVX.exe`, `AMS2.exe`, `pCARS2AVX.exe`, `pCARS2.exe` | Enable **UDP Frequency > 0** in game settings |
| `wreckfest2` | Wreckfest 2 | 23123 | `Wreckfest2.exe` | Telemetry sent automatically |
| `beamng-outgauge` | BeamNG.drive (OutGauge) | 63392 | `BeamNG.drive.exe` | Options > Other > OutGauge > enable, IP `127.0.0.1`, port `63392` |
| `beamng-sh` | BeamNG.drive (SimHub Mod) | 9999 | `BeamNG.drive.exe` | Requires the SimHub telemetry mod installed in BeamNG |

```
sim-relay list
```
prints this table with setup notes.

### Not Supported

| Game | Reason |
|------|--------|
| **Assetto Corsa EVO** | Uses shared memory (not UDP). Planned: shared-memory forwarding like iracing-teleport. |
| **Assetto Corsa (original)** | Stateful handshake UDP — the game's UDP server requires a subscribe/response protocol that cannot be transparently proxied. Use shared-memory mirroring instead. |
| **Assetto Corsa Rally** | Likely shared memory (research inconclusive on UDP exposure). |
| **Microsoft Flight Simulator 2024** | Uses the SimConnect SDK (named pipe / TCP), not UDP. Requires a dedicated SimConnect relay tool. |

---

## Quick Start

### Direct Ethernet (192.168.50.1 source, 192.168.50.2 target)

**Gaming PC (source):**
```
sim-relay source --target 192.168.50.2
```
Binds all supported game ports and forwards every packet to the target machine.

**SimHub PC (target):**  
SimHub listens on the game's default port natively — no sim-relay needed on the
target if SimHub is already configured for the right port. Just point the games to
their default ports and SimHub will receive the relayed packets.

If you need to run sim-relay on the target (e.g. to remap to a different local port):
```
sim-relay target --source 192.168.50.1
```

### LAN Setup

Same commands — just use the actual LAN IP of the target machine:
```
sim-relay source --target 192.168.1.42
```

---

## Firewall Rules (Target PC)

Windows Firewall blocks inbound UDP by default. Allow the game ports on the target PC:

```powershell
# Run as Administrator
netsh advfirewall firewall add rule name="sim-relay pcars2" dir=in action=allow protocol=UDP localport=5606
netsh advfirewall firewall add rule name="sim-relay wreckfest2" dir=in action=allow protocol=UDP localport=23123
netsh advfirewall firewall add rule name="sim-relay beamng-outgauge" dir=in action=allow protocol=UDP localport=63392
netsh advfirewall firewall add rule name="sim-relay beamng-sh" dir=in action=allow protocol=UDP localport=9999
```

Or allow all four at once:
```powershell
netsh advfirewall firewall add rule name="sim-relay all" dir=in action=allow protocol=UDP localport=5606,23123,63392,9999
```

---

## Running Alongside Local SimHub (Source PC)

If SimHub is also installed on the gaming PC, it normally binds the same UDP port as
the game. When sim-relay source binds that port, only one of them receives each
packet (Windows delivers UDP unicast to the most recently bound socket with
`SO_REUSEADDR`).

**Option A — Use `--local-forward`:**
sim-relay source forwards each packet to `localhost:<port+1000>` in addition to the
remote target. Configure SimHub on the source PC to listen on that offset port
(e.g. pcars2 → 6606 instead of 5606).

```
sim-relay source --target 192.168.50.2 --local-forward
```

**Option B — Don't run SimHub on the source PC.**  
This is the cleaner setup: game data stays on the gaming PC, SimHub lives on its own
machine.

---

## Options Reference

### `sim-relay source`

| Flag | Description |
|------|-------------|
| `--target <IP>` | **Required.** Target PC IP address. |
| `--games <id,...>` | Comma-separated game IDs (default: all). |
| `--all` | Forward all supported games. |
| `--local-forward` | Also forward to `localhost:<port+1000>` for a local SimHub. |
| `--bind <IP>` | Bind address for listen sockets (default: `0.0.0.0`). |
| `--high-priority` | Set `HIGH_PRIORITY_CLASS` for this process. |
| `--auto-detect` | Only bind a port when that game's process is detected running. Frees the port when the game closes. |

### `sim-relay target`

| Flag | Description |
|------|-------------|
| `--source <IP>` | Source PC IP (informational). |
| `--games <id,...>` | Game IDs to listen for (default: all). |
| `--all` | Listen on all supported game ports. |
| `--forward-to <IP:PORT>` | Override forwarding destination (default: `127.0.0.1:<game_port>`). |
| `--high-priority` | Set `HIGH_PRIORITY_CLASS`. |
| `--busy-wait` | Spin on recv instead of sleeping (lower latency, higher CPU). |

### `sim-relay list`

Prints all supported games with ports and setup notes.

---

## Auto-Detect Mode

With `--auto-detect`, sim-relay source polls the running process list every 5 seconds.
It only binds a game's UDP port when that game's executable is detected, and releases
the port when the process disappears. This prevents port conflicts with other apps
when the game isn't running.

```
sim-relay source --target 192.168.50.2 --auto-detect
```

Process detection is Windows-only. On other platforms, `--auto-detect` has no effect
(ports are bound at startup as usual).

---

## Port Conflict Notes

sim-relay source binds game ports with `SO_REUSEADDR`. On Windows, when two sockets
are bound to the same UDP port with `SO_REUSEADDR`, only one receives each incoming
unicast packet (behaviour varies by Windows version but is generally last-bound wins).
This means sim-relay source and SimHub cannot simultaneously receive the same game
packets on the same port — use `--local-forward` if you need both.

On the target PC, sim-relay target and SimHub CAN share a port with `SO_REUSEADDR` if
started in the right order (start sim-relay target first, SimHub second). Alternatively,
use `--forward-to` to point sim-relay target at a different port and configure SimHub
accordingly.

---

## Building

```
cargo build --release -p sim-relay
```

The binary will be at `target/release/sim-relay.exe` (Windows) or `target/release/sim-relay`.

---

## Architecture

```
  Gaming PC (source)                     Target PC

  ┌──────────────┐                   ┌──────────────┐
  │  Game         │                   │  SimHub       │
  │  sends UDP    │                   │  listens on   │
  │  to localhost │                   │  game port    │
  │  :5606        │                   │  :5606        │
  └──────┬───────┘                   └──────▲───────┘
         │                                  │
  ┌──────▼───────┐     UDP packet     ┌─────┴────────┐
  │  sim-relay    │ ──────────────►   │  sim-relay    │
  │  source       │   (raw bytes)     │  target       │
  │  binds :5606  │                   │  (optional)   │
  │  forwards to  │                   │  forwards to  │
  │  target:5606  │                   │  localhost    │
  └──────────────┘                   └──────────────┘
```

No compression, no framing, no state — raw UDP bytes in, raw UDP bytes out.
Packet sizes are well under MTU (typically 96–2000 bytes), so no fragmentation.

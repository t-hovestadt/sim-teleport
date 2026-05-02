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

**Companion to [iracing-teleport](../teleport/)**, which handles iRacing via shared
memory. sim-relay covers games that broadcast UDP telemetry natively.

---

## Download

Pre-built Windows x64 binary is on the [Releases](../../releases/latest) page.

| File | Use |
|------|-----|
| `sim-relay-vX.X.X-windows-x86_64.exe` | Both gaming PC (source) and SimHub PC (target) |

---

## Quick Start

**Gaming PC (source):**
```
sim-relay.exe source --target <SimHub-PC-IP>
```
Binds all supported game ports and forwards every packet to the target machine.

**SimHub PC (target — optional):**

SimHub listens on game ports natively. You only need sim-relay on the target if SimHub
is configured for a different port or you want to remap:
```
sim-relay.exe target
```

**List supported games:**
```
sim-relay.exe list
```

**Direct Ethernet (192.168.50.1 → 192.168.50.2):** See the [Direct Ethernet setup](#direct-ethernet-setup) section.

---

## Supported Games

| ID | Game | Port | Detection | Setup |
|----|------|------|-----------|-------|
| `pcars2` | Project Cars 2 / Automobilista 2 | 5606 | `AMS2AVX.exe`, `AMS2.exe`, `pCARS2AVX.exe`, `pCARS2.exe` | Enable **UDP Frequency > 0** in game settings |
| `wreckfest2` | Wreckfest 2 | 23123 | `Wreckfest2.exe` | Telemetry sent automatically |
| `beamng-outgauge` | BeamNG.drive (OutGauge) | 63392 | `BeamNG.drive.exe` | Options → Other → OutGauge → enable, IP `127.0.0.1`, port `63392` |
| `beamng-sh` | BeamNG.drive (SimHub Mod) | 9999 | `BeamNG.drive.exe` | Requires the SimHub telemetry mod installed in BeamNG |

`sim-relay list` prints this table with setup notes.

### Not Supported

| Game | Reason |
|------|--------|
| **Assetto Corsa EVO** | Uses shared memory (not UDP). Planned: shared-memory forwarding like iracing-teleport. |
| **Assetto Corsa (original)** | Stateful handshake UDP — the game's UDP server requires a subscribe/response protocol that cannot be transparently proxied. |
| **Assetto Corsa Rally** | Likely shared memory (research inconclusive on UDP exposure). |
| **Microsoft Flight Simulator 2024** | Uses SimConnect SDK (named pipe / TCP), not UDP. Requires a dedicated SimConnect relay. |

---

## Options

| Flag | source | target | Default | Description |
|------|:------:|:------:|---------|-------------|
| `--target <IP>` | ✓ | | — | **Required for source.** Target PC IP address |
| `--games <id,...>` | ✓ | ✓ | (all) | Comma-separated game IDs to forward/receive |
| `--all` | ✓ | ✓ | off | Forward/listen on all supported games |
| `--local-forward` | ✓ | | off | Also forward to `localhost:<port+1000>` for a local SimHub instance |
| `--bind <IP>` | ✓ | | `0.0.0.0` | Bind address for listen sockets |
| `--high-priority` | ✓ | ✓ | off | Raise process to `HIGH_PRIORITY_CLASS` for lower scheduling jitter |
| `--auto-detect` | ✓ | | off | Only bind a port when that game's process is detected running; releases port when game closes |
| `--source <IP>` | | ✓ | — | Source PC IP (informational only) |
| `--forward-to <IP:PORT>` | | ✓ | `127.0.0.1:<game_port>` | Override forwarding destination |
| `--busy-wait` | | ✓ | off | Spin on recv instead of sleeping (lower latency, higher CPU) |

---

## How It Works

- **source** binds each game's default UDP port with `SO_REUSEADDR`, reads incoming packets
  in a non-blocking drain loop, and re-transmits raw bytes to the target PC. No parsing, no
  modification — SimHub receives the same bytes the game sent.
- **target** (optional) listens on the same ports and forwards to `127.0.0.1:<port>` so SimHub
  on the target PC receives packets on the port it expects. A separate forwarding socket is used
  to prevent loopback self-receive. Useful if SimHub is configured for a non-default port.
- **Drain loop**: both source and target drain all queued packets from each socket per iteration
  before sleeping 100 µs. This prevents buffer buildup under burst conditions (e.g. 300 pkt/s
  from pcars2).

Both tools print a stats line every 5 s and a summary on Ctrl-C:

```
[Project Cars 2 / Automobilista 2]  60.1 pkt/s   72.5 KB/s   avg 1205 b/pkt   3 µs fwd
[Wreckfest 2]  inactive
```

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
New-NetFirewallRule -DisplayName "Sim Relay" -Direction Inbound -Protocol UDP -LocalPort 5606,9999,23123,63392 -Action Allow
```

Or via *Windows Defender Firewall → Advanced Settings → Inbound Rules → New Rule → Port → UDP → 5606,9999,23123,63392 → Allow*.

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

`start-source-all.bat` on the **gaming PC** — place it next to `sim-relay.exe`:

```batch
@echo off
cd /d "%~dp0"
sim-relay.exe source --target 192.168.50.2 --all
pause
```

`start-target-all.bat` on the **SimHub PC**:

```batch
@echo off
cd /d "%~dp0"
sim-relay.exe target --all
pause
```

Pre-built bat files for common setups are included in the release download.

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
sim-relay.exe source --target 192.168.50.2 --all --local-forward
```

**Option B — Run SimHub only on the target PC.** Cleanest setup.

---

## Auto-Detect Mode

With `--auto-detect`, sim-relay source polls the running process list every 5 seconds. It only
binds a game's UDP port when that game's executable is detected, and releases the port when the
process disappears. This prevents port conflicts with other apps when the game isn't running.

```
sim-relay.exe source --target 192.168.50.2 --all --auto-detect
```

Process detection is Windows-only. On other platforms `--auto-detect` has no effect — ports are
bound at startup as usual.

---

## Port Conflict Notes

sim-relay source binds game ports with `SO_REUSEADDR`. On Windows, when two sockets are bound to
the same UDP port, only one receives each unicast packet (generally last-bound wins). This means
sim-relay source and SimHub cannot simultaneously receive the same game packets — use
`--local-forward` if you need both.

On the target PC, sim-relay target and SimHub can share a port with `SO_REUSEADDR` if started in
the right order (start sim-relay target first, SimHub second). Alternatively use `--forward-to`
to point sim-relay target at a different port and configure SimHub accordingly.

---

## License

MIT

# ac-teleport

Stream Assetto Corsa (AC1), Assetto Corsa Competizione (ACC), or Assetto Corsa EVO
telemetry over your local network so SimHub (or any compatible app) runs on a
separate machine from your game PC. Small Windows executables, no installers, no
dependencies.

```
┌─────────────────────────┐         UDP (multicast or unicast)        ┌─────────────────────────┐
│     Game PC             │  ────────────────────────────────────►   │     SimHub PC           │
│                         │                                           │                         │
│  AC1 / ACC / ACE        │                                           │  SimHub / overlays      │
│    └─ shared memory     │                                           │    └─ shared memory     │
│         └─ source       │                                           │         └─ target       │
└─────────────────────────┘                                           └─────────────────────────┘
```

**For all games in one app:** [sim-bridge](https://github.com/t-hovestadt/sim-bridge)
bundles AC Teleport with iRacing Teleport and Sim Relay. One binary,
automatic game detection.

**Companion projects:**
- [iracing-teleport](https://github.com/t-hovestadt/iracing-teleport) — iRacing (shared memory)
- [sim-relay](https://github.com/t-hovestadt/sim-relay) — games that broadcast UDP natively
- [sim-bridge](https://github.com/t-hovestadt/sim-bridge) — unified single-binary launcher for all three

---

## Download

Pre-built Windows x64 binaries are on the [Releases](../../releases/latest) page.

| File | Machine | Notes |
|------|---------|-------|
| `source.exe` | Game PC | Run directly: `source.exe` |
| `target.exe` | SimHub PC | Run directly: `target.exe` |
| `ac-teleport.exe` | Both | Combined CLI: `ac-teleport.exe source` / `ac-teleport.exe target` |

---

## Windows SmartScreen

On first run, Windows may show "Windows protected your PC." This is normal for unsigned open-source software.

To unblock: right-click the `.exe` → **Properties** → check **Unblock** at the bottom of the General tab → **OK**.

Or click **More info** on the SmartScreen dialog, then **Run anyway**.

---

## Quick start

**Default (multicast — works on most home networks):**

1. Run on your **SimHub PC**:
   ```
   ac-teleport.exe target
   ```
2. Run on your **Game PC**:
   ```
   ac-teleport.exe source
   ```

Start them in any order. Source auto-detects which game is running. Target creates
maps for all supported games simultaneously and writes incoming data to each.

**Unicast (if multicast doesn't work on your network):**

```
# SimHub PC
ac-teleport.exe target --unicast

# Game PC (replace with your SimHub machine's IP)
ac-teleport.exe source --unicast --target 192.168.1.50:5001
```

**Direct Ethernet (point-to-point cable between the two PCs):**

See [Direct Ethernet setup](#direct-ethernet-setup) below.

---

## Supported games

| `--game` flag | Game | Shared memory maps |
|--------------|------|--------------------|
| `ac1` | Assetto Corsa | `Local\acpmf_physics`, `Local\acpmf_graphics`, `Local\acpmf_static` |
| `evo` | Assetto Corsa EVO | `Local\acevo_pmf_physics`, `Local\acevo_pmf_graphics`, `Local\acevo_pmf_static` |
| *(auto)* | Either (auto-detect) | Both sets on target |

**ACC note:** Assetto Corsa Competizione uses the same `Local\acpmf_*` names as AC1.
Source detects and forwards ACC telemetry — the target's AC1 maps receive it. ACC's
primary telemetry interface is its own UDP broadcasting protocol on port 9000; the
shared-memory path is an unofficial/experimental channel.

---

## Game detection

**Source auto-detection** (default, no `--game` flag): every 2 seconds, source
probes for EVO maps then AC1 maps by opening test handles and immediately closing
them. No handles are held between polls — maps don't persist after a game exits.
EVO is probed first; if both are running simultaneously, EVO wins.

**Game switching**: if you close EVO and launch AC1 without restarting ac-teleport,
it handles this automatically. Source detects the old game's `packetId` stopped
advancing (or maps closed), drops handles, re-enters the detection loop, and picks
up the new game within 2 seconds.

**Managed mode** (when embedded in sim-bridge): sim-bridge selects the game based
on its own detection logic and passes the result to ac-teleport as `game=Some(...)`.
In managed mode the `packetId`-based stale timeout is disabled — source stays
connected through menu screens and loading screens. Only an explicit shutdown signal
(game process exited, detected by sim-bridge's scanner) causes a disconnect. This
prevents spurious disconnect/reconnect thrashing while a player is in the game menu.

---

## Options

| Flag | source | target | Default | Description |
|------|:------:|:------:|---------|-------------|
| `--game <ac1\|evo>` | ✓ | ✓ | *(auto)* | Force a specific game; omit for auto-detect |
| `--bind <ADDR>` | ✓ | ✓ | `0.0.0.0:0` / `0.0.0.0:5001` | Local address to bind the UDP socket |
| `--target <ADDR>` | ✓ | | `239.255.0.1:5001` | Destination (multicast group:port or unicast IP:port) |
| `--unicast` | ✓ | ✓ | off | Direct host-to-host (no multicast) |
| `--group <ADDR>` | | ✓ | `239.255.0.1` | Multicast group to join |
| `--busy-wait` | ✓ | ✓ | off | Spin instead of sleeping (~0–2 ms less jitter, one CPU core) |
| `--poll-rate <HZ>` | ✓ | | `60` | Page polling rate (Hz) |
| `--pin-core <N>` | ✓ | ✓ | off | Pin the worker thread to CPU core N (0-based) |
| `--stale-timeout <SECS>` | | ✓ | `10` | Seconds without data before zeroing game status |
| `--high-priority` | ✓ | ✓ | off | `HIGH_PRIORITY_CLASS` + `ABOVE_NORMAL` thread priority. Safe on SimHub PC; avoid on game PC. |
| `--verbose` | ✓ | | off | Print first 64 bytes of EVO physics/graphics maps every second as hex (useful for debugging `packetId` offset in new game versions) |

---

## How it works

### Source

Source polls three shared-memory pages at `--poll-rate` Hz. Each page has a
`packetId` counter (i32) at byte offset 0. When `packetId` changes, the page is
LZ4-compressed and sent over UDP. Pages that haven't changed are not sent.

The entire pipeline is a raw byte tunnel — source reads the actual map size via
`VirtualQuery` (OS-reported, not a hardcoded constant), compresses the entire
byte slice, and sends it. No struct parsing, no field interpretation. This makes
the pipeline version-agnostic: EVO v0.6 changed the struct layout, and
ac-teleport required no changes — the bytes flow through unchanged, and SimHub
on the target reads them using its own struct definitions.

**Heartbeat packets** are sent when the game is on the menu or loading
(`AC_STATUS` at offset 4 in the graphics page is 0, meaning AC_OFF). The
heartbeat is a header-only packet that resets the target's stale timer without
decompressing — this keeps the shared maps alive between sessions.

**Game announce**: immediately after detecting a game (and every 30 seconds),
source sends a `PAGE_GAME_ANNOUNCE` packet (buf_offset = `0xFFFFFFFE`) with a
1-byte payload: `0` = AC1, `1` = EVO, `2` = ACC. Old targets that don't
recognize this buf_offset skip it silently (the existing `page_idx > 2` guard)
— fully backward-compatible.

### Target

Target creates six named shared-memory regions at startup — three maps for EVO
and three for AC1 — each 64 KB (`DUAL_MAP_SIZE`):

```
Local\acevo_pmf_physics      Local\acpmf_physics
Local\acevo_pmf_graphics     Local\acpmf_graphics
Local\acevo_pmf_static       Local\acpmf_static
```

Every incoming telemetry page is written to both EVO and AC1 maps simultaneously.
SimHub reads the set matching its configured game; the other set sits idle.

On stale timeout (no data for `--stale-timeout` seconds), target zeroes
`AC_STATUS` (offset 4 in the graphics page) in both map sets. This signals SimHub
there is no active session without destroying the maps.

Maps are created with a NULL DACL (explicit "all access") so SimHub and other apps
can open them regardless of elevation or user account — matching what the game itself
does.

---

## Wire protocol

Each UDP datagram carries a 24-byte header (`repr(C, packed)`, little-endian):

| Field | Type | Description |
|-------|------|-------------|
| `source_us` | u64 | Source-side processing time in microseconds |
| `sequence` | u32 | Monotonically increasing per message |
| `payload_size` | u32 | Total LZ4-compressed bytes across all fragments |
| `buf_offset` | u32 | Page: `0`=physics, `1`=graphics, `2`=static; `u32::MAX`=heartbeat; `0xFFFFFFFE`=game announce |
| `fragment` | u16 | 0-based fragment index |
| `fragments` | u16 | Total fragment count; `0` = heartbeat (no payload) |

**Game announce datagram**: `buf_offset = 0xFFFFFFFE`, `fragments = 1`,
1-byte payload (game_id: 0=AC1, 1=EVO, 2=ACC).

The receiver reassembles fragments out-of-order and discards duplicates. A new
sequence clears any in-progress assembly from the previous one.

---

## Compatible apps

Any app that reads AC1 or ACE shared memory works on the target machine —
the maps are identical to what the game produces locally.

- [SimHub](https://www.simhubdash.com) — dashboards, overlays, haptics, LED control
- [Crew Chief](https://thecrewchief.org) — AI spotter and engineer with voice feedback
- Any app polling `Local\acpmf_*` (AC1/ACC) or `Local\acevo_pmf_*` (ACE)

---

## Direct Ethernet setup

A direct Ethernet cable between the two PCs gives the lowest possible latency.

**1. Assign static IPs**

| PC | IP | Subnet |
|----|-----|--------|
| Game PC | `192.168.50.1` | `255.255.255.0` |
| SimHub PC | `192.168.50.2` | `255.255.255.0` |

In Windows: *Network & Internet → Change adapter options → right-click adapter →
Properties → IPv4 → Use the following IP address*. Leave gateway and DNS blank.

**2. Firewall rules** (run as Administrator)

On the **Game PC** (receives heartbeats from target):
```powershell
New-NetFirewallRule -DisplayName "AC Teleport source" `
    -Direction Inbound -Protocol UDP -LocalPort 5001 -Action Allow
```

On the **SimHub PC** (receives telemetry from game PC):
```powershell
New-NetFirewallRule -DisplayName "AC Teleport target" `
    -Direction Inbound -Protocol UDP -LocalPort 5001 -Action Allow
```

**3. NIC settings (both PCs)**

Device Manager → Network Adapters → right-click the direct-link adapter →
Properties:

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

`start-source.bat` on the **Game PC**:
```batch
@echo off
cd /d "%~dp0"
ac-teleport.exe source --unicast --target 192.168.50.2:5001 --bind 192.168.50.1:5001
pause
```

`start-target.bat` on the **SimHub PC**:
```batch
@echo off
cd /d "%~dp0"
ac-teleport.exe target --unicast --bind 192.168.50.2:5001
pause
```

**Troubleshooting**

*Adapter shows Disconnected:* Full Shut down (not Restart), wait 30–60 s, power on.
Disable Wake-on-LAN in NIC settings and BIOS.

*Link won't establish:* Set Speed & Duplex to 1.0 Gbps Full Duplex; confirm Auto
MDI/MDIX is Auto.

*Can't set static IP:* Plug cable in first (adapter needs a link), then set IP.
To reset: `Remove-NetIPAddress -InterfaceIndex <N> -Confirm:$false`.

---

## Library API

ac-teleport is a library crate (`ac-teleport = { path = "…" }`). The public API
surface used by sim-bridge:

```rust
// Game configurations
pub struct GameConfig {
    pub id: &'static str,           // "ac1" or "evo"
    pub name: &'static str,
    pub physics_map: &'static str,  // e.g., "Local\\acevo_pmf_physics"
    pub graphics_map: &'static str,
    pub static_map: &'static str,
    pub max_physics_size: usize,    // informational; DUAL_MAP_SIZE dominates
    pub max_graphics_size: usize,
    pub max_static_size: usize,
}

pub fn resolve(id: &str) -> Option<&'static GameConfig>;  // "ac1" or "evo"

// Game ID constants (PAGE_GAME_ANNOUNCE payload)
pub const GAME_ID_AC1: u8 = 0;
pub const GAME_ID_EVO: u8 = 1;
pub const GAME_ID_ACC: u8 = 2;

// Source
pub struct SourceArgs {
    pub game: Option<&'static GameConfig>,  // None = auto-detect
    pub target: String,
    pub bind: String,
    pub unicast: bool,
    pub busy_wait: bool,
    pub pin_core: Option<usize>,
    pub high_priority: bool,
    pub poll_rate: u32,
    pub verbose: bool,
}

pub mod source {
    pub fn run(args: SourceArgs, shutdown: Receiver<()>) -> anyhow::Result<()>;
}

// Target
pub struct TargetArgs {
    pub game: Option<&'static GameConfig>,  // None = dual-map mode
    pub bind: String,
    pub group: String,
    pub unicast: bool,
    pub busy_wait: bool,
    pub pin_core: Option<usize>,
    pub high_priority: bool,
    pub stale_timeout: Duration,
    pub on_first_data: Option<Arc<dyn Fn() + Send + Sync>>,
    pub on_stale: Option<Arc<dyn Fn() + Send + Sync>>,
    pub on_game_announce: Option<Arc<dyn Fn(u8) + Send + Sync>>,
}

pub mod target {
    pub fn run(args: TargetArgs, shutdown: Receiver<()>) -> anyhow::Result<()>;
}
```

The `on_game_announce` callback receives the game ID byte (0/1/2) before the first
telemetry frame arrives. sim-bridge uses this to spawn the correct stub process
(`acs.exe` vs `assettocorsa_evo.exe` vs `acc.exe`) and call
`SimHubWPF.exe -switchgame` with the right game code.

---

## Building from source

Requires [Rust](https://rustup.rs) (stable) and a Windows x64 target.

```
git clone https://github.com/t-hovestadt/ac-teleport
cd ac-teleport
cargo build --release
```

Cross-compile for Windows from macOS or Linux:

```
rustup target add x86_64-pc-windows-gnu
brew install mingw-w64   # macOS
CARGO_TARGET_DIR=/tmp/ac-build cargo build --release --target x86_64-pc-windows-gnu
```

If your working directory path contains spaces, set `CARGO_TARGET_DIR` to a
path without spaces — the `mingw-w64` linker doesn't handle quoted paths.

`#[cfg(windows)]` code is invisible to macOS/Linux clippy. Always run:
```
cargo clippy --target x86_64-pc-windows-gnu -- -D warnings
```
before pushing any Windows-specific changes.

---

<details>
<summary>Technical details</summary>

### Change detection

Source reads `packetId` (i32 at byte offset 0) from the physics and graphics
maps every tick. A page is only compressed and sent when its `packetId` changes.
Physics updates at ~333 Hz and graphics at ~60 Hz; both are captured at the
configured `--poll-rate`. The static page is resent every 10 seconds as a fallback
in addition to change detection.

If `packetId` at offset 0 is not advancing (e.g., because a game update moved the
field), use `--verbose` to dump the first 64 bytes of EVO maps every second as hex,
then identify which 4-byte group increments each frame. The `--verbose` output format:

```
[acevo-diag] physics[0..64]:  00 00 00 00 01 00 00 00 ...
[acevo-diag] graphics[0..64]: 00 00 00 00 00 00 00 00 ...
```

### Compression

Pages are compressed with LZ4 (`lz4_flex` crate). Buffer sizes are computed
from `get_maximum_output_size(map_size)` at runtime. The maximum UDP datagram
size is 9,000 bytes (8,976-byte payload after the 24-byte header). Large pages
are fragmented; the receiver reassembles out-of-order.

Compression buffers are allocated once at startup (per-game size); no heap
allocation on the hot path.

### Dual-map size

Target creates all maps at `DUAL_MAP_SIZE = 65536` (64 KB), regardless of game
or reported struct size. This is safely larger than all current and foreseeable
AC struct sizes and leaves room for future growth.

### Performance

- **2 MB socket buffers** on both sides — the OS default (64 KB on Windows) is
  smaller than a single uncompressed EVO graphics page.
- **1 ms timer resolution** — `timeBeginPeriod(1)` ensures sleep/wait
  calls resolve at 1 ms rather than 15.6 ms default.
- **MMCSS on target** — registered under "Games" multimedia task for reserved
  CPU time. Not applied to source.
- **NULL DACL shared memory** — maps created with all-access security descriptor.
- **Fragment validation** — header fields are checked before processing; malformed
  or spoofed packets are silently discarded.

Release profile uses LTO, a single codegen unit, and symbol stripping.

</details>

---

## License

MIT

# ac-teleport

Stream **Assetto Corsa (AC1)** or **Assetto Corsa EVO** telemetry over your local network so
SimHub (or any compatible app) runs on a separate machine from your game PC. Small Windows
executables, no installers, no dependencies.

```
┌─────────────────────────┐         UDP (multicast or unicast)        ┌─────────────────────────┐
│     Game PC             │  ────────────────────────────────────►   │     SimHub PC           │
│                         │                                           │                         │
│  AC1 / ACE              │                                           │  SimHub / overlays      │
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

Start them in any order. source auto-detects which game is running; target creates maps for both
games simultaneously. Launch AC1 or AC EVO — ac-teleport picks it up automatically.

**Unicast (if multicast doesn't work on your network):**

```
# SimHub PC
ac-teleport.exe target --unicast

# Game PC (replace with your SimHub machine's IP)
ac-teleport.exe source --unicast --target 192.168.1.50:5001
```

**Direct Ethernet (point-to-point cable between the two PCs):**

See the [Direct Ethernet setup](#direct-ethernet-setup) section below.

---

## Game detection

ac-teleport automatically detects which Assetto Corsa game is running. No `--game` flag needed.

**How it works on source:** Every 2 seconds, source probes the shared memory names for EVO, then
AC1. It checks by attempting to open each map handle and immediately closing it — no handles
are held while waiting, so maps don't persist after a game exits. When all three maps for a
game are found, source opens them for real and starts forwarding.

**Detection order:** EVO is checked before AC1. If both are running simultaneously (unlikely),
EVO wins. Use `--game ac1` or `--game evo` to force a specific game.

**Game switching:** If you close AC EVO and launch AC1 (or vice versa) without restarting
ac-teleport, it handles this automatically:
1. Source detects EVO's packetIds stopped advancing (game closed)
2. Source drops EVO handles, returns to detection loop
3. Source detects AC1 maps, connects, resumes forwarding

**How it works on target:** Target creates shared memory maps for **both** EVO and AC1 at startup
(six maps total). Every incoming page is written to both sets. SimHub reads the set matching
the game it is configured for — the other set just sits idle. No protocol changes are needed.

**ACC note:** Assetto Corsa Competizione (ACC) uses the same shared memory names as AC1
(`Local\acpmf_*`). Source will detect and forward ACC single-player telemetry. This is
unofficial / experimental — ACC's primary telemetry interface is its own UDP broadcasting
protocol on port 9000.

---

## Supported games

| Flag | Game | Shared memory prefix |
|------|------|----------------------|
| `--game ac1` | Assetto Corsa | `Local\acpmf_*` |
| `--game evo` | Assetto Corsa EVO | `Local\acevo_pmf_*` |
| *(default)* | Auto-detect | Both sets created on target |

> **For iRacing**, use [iracing-teleport](https://github.com/t-hovestadt/iracing-teleport) instead — it streams iRacing's single
> shared memory region, which has a completely different structure.

---

## Options

| Flag | source | target | Default | Description |
|------|:------:|:------:|---------|-------------|
| `--game <ac1\|evo>` | ✓ | ✓ | *(auto-detect)* | Force a specific game; omit for auto-detect |
| `--bind <ADDR>` | ✓ | ✓ | `0.0.0.0:0` / `0.0.0.0:5001` | Local address to bind the UDP socket to |
| `--target <ADDR>` | ✓ | | `239.255.0.1:5001` | Destination (multicast group:port or unicast IP:port) |
| `--unicast` | ✓ | ✓ | off | Send/receive directly host-to-host instead of multicast |
| `--group <ADDR>` | | ✓ | `239.255.0.1` | Multicast group to join |
| `--busy-wait` | ✓ | ✓ | off | Spin instead of sleeping; eliminates OS scheduler wake-up jitter (~0–2 ms), costs one CPU core |
| `--poll-rate <HZ>` | ✓ | | `60` | Page polling rate in Hz |
| `--pin-core <N>` | ✓ | ✓ | off | Pin the worker thread to CPU core N (0-based) |
| `--stale-timeout <SECS>` | | ✓ | `10` | Seconds without data before action: drop maps (single-game mode) or zero game status (dual mode) |
| `--high-priority` | ✓ | ✓ | off | Raise to HIGH_PRIORITY_CLASS + ABOVE_NORMAL thread priority. Safe on the SimHub PC; on the game PC only use if the game is not running on the same machine |

---

## How it works

- **source** polls three named shared-memory pages at `--poll-rate` Hz. Each page has a `packetId` counter at offset 0; when it changes, source compresses that page with LZ4 and sends it over UDP. In auto-detect mode source probes for EVO then AC1 maps every 2 s until one is found. It reconnects automatically if the game closes and re-detects whichever game starts next.
- **target** receives the UDP stream, reassembles fragments, decompresses, and writes the data into matching shared-memory pages on the SimHub PC — so SimHub sees the game as if it were installed locally. In dual-map mode (default) maps for both games are created at startup and data is written to both sets simultaneously.
- **Heartbeats** keep the connection alive across menus and loading screens so SimHub doesn't disconnect.

Both tools print a stats line every 5 s and a summary on Ctrl-C:

```
[source] 60.0 msg/s  0.12 Mbps  4.2x  42 µs avg  1.0 frags/msg  0 dropped
[target] 60.0 msg/s  0.12 Mbps  4.2x  38 µs avg  1.0 frags/msg  0 dropped
```

The `4.2x` figure is the compression ratio (uncompressed ÷ compressed bytes).

---

## Compatible apps

Any app that reads AC1 or ACE shared memory works automatically on the target machine —
the memory maps are identical to what the game produces locally.

- [SimHub](https://www.simhubdash.com) — dashboards, overlays, haptics, LED control
- [Crew Chief](https://thecrewchief.org) — AI spotter and engineer with voice feedback
- Other apps that poll `Local\acpmf_*` (AC1) or `Local\acevo_pmf_*` (ACE)

---

## Direct Ethernet setup

A direct Ethernet cable between the two PCs (no router, no switch) gives the lowest possible
latency. You need:

- A network adapter on each PC
- A Cat 5e or better Ethernet cable
- Static IP addresses (Windows won't auto-assign usable IPs on a direct link)

**1. Assign static IPs**

On each PC, set a static IP on the direct-link adapter:

| PC | IP | Subnet |
|----|-----|--------|
| Game PC | `192.168.50.1` | `255.255.255.0` |
| SimHub PC | `192.168.50.2` | `255.255.255.0` |

In Windows: *Network & Internet → Change adapter options → right-click adapter → Properties → IPv4 → Use the following IP address*. Leave gateway and DNS blank.

**2. Firewall rules**

Run the following in PowerShell (Administrator).

**On the Game PC** (receives resync packets from the SimHub PC):

```powershell
New-NetFirewallRule -DisplayName "AC Teleport source" `
    -Direction Inbound -Protocol UDP -LocalPort 5001 -Action Allow
```

**On the SimHub PC** (receives telemetry from the Game PC):

```powershell
New-NetFirewallRule -DisplayName "AC Teleport target" `
    -Direction Inbound -Protocol UDP -LocalPort 5001 -Action Allow
```

Or via *Windows Defender Firewall → Advanced Settings → Inbound Rules → New Rule → Port → UDP → 5001 → Allow* on each PC.

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

Use the provided bat files (edit the paths to match where you placed `ac-teleport.exe`):

- `start-source.bat` — Game PC (auto-detects AC1 or AC EVO)
- `start-target.bat` — SimHub PC (creates maps for both games)

**Troubleshooting**

*Adapter shows Disconnected despite cable plugged in:* Wake-on-LAN or PCIe ASPM can leave the NIC in a state a warm reboot doesn't clear. Do a full **Shut down** (not Restart), wait 30–60 seconds, then power on. To prevent recurrence: disable Wake-on-LAN in the NIC settings above and in BIOS.

*Link won't establish between two NICs:* Some NIC brands fail auto-negotiation on a direct connection. The Speed & Duplex setting above (1.0 Gbps Full Duplex) fixes this. Also confirm **Auto MDI/MDIX** is Auto — if disabled, a straight-through cable won't link up without a crossover cable.

*Can't set static IP via PowerShell (`element not found` or `already exists`):* Plug the cable in first so the adapter shows a link, then set the IP. To reset: `Remove-NetIPAddress -InterfaceIndex <N> -Confirm:$false` then re-add.

---

## Building from source

Requires [Rust](https://rustup.rs) (stable) and a Windows x64 target.

```
git clone https://github.com/t-hovestadt/ac-teleport
cd ac-teleport
cargo build --release
```

The binary is at `target/release/ac-teleport.exe`.

Cross-compiling for Windows from macOS or Linux requires `mingw-w64` and the `x86_64-pc-windows-gnu` Rust target:

```
rustup target add x86_64-pc-windows-gnu
brew install mingw-w64          # macOS
CARGO_TARGET_DIR=/tmp/ac-build cargo build --release --target x86_64-pc-windows-gnu
```

> If your working directory path contains spaces, set `CARGO_TARGET_DIR` to a path without spaces (the `mingw-w64` linker doesn't handle quoted paths).

---

<details>
<summary>Technical details</summary>

### Protocol

Each page is compressed with LZ4 and split into 9,000-byte UDP datagrams. Every datagram
carries a 24-byte header:

| Field | Type | Description |
|-------|------|-------------|
| `source_us` | u64 | Microseconds spent on source side |
| `sequence` | u32 | Monotonically increasing per message |
| `payload_size` | u32 | Total compressed bytes across all fragments |
| `buf_offset` | u32 | Page identifier: `0`=physics, `1`=graphics, `2`=static, `u32::MAX`=heartbeat |
| `fragment` | u16 | 0-based index of this fragment |
| `fragments` | u16 | Total fragment count for this sequence; `0` = heartbeat |

The receiver reassembles fragments out-of-order and discards duplicates. A new sequence discards any in-progress assembly from the previous one.

There is no game-type field in the protocol. The target writes every incoming page into both
EVO and AC1 maps. This keeps the wire format stable regardless of which game is running.

### Page polling

Source reads `packetId` (i32 at byte offset 0) from each page every tick. A page is only
compressed and sent when its `packetId` changes. Physics updates at ~333 Hz and graphics at
~60 Hz; both are captured at the configured `--poll-rate`. Static data is resent every 10 s
as a fallback in addition to change detection.

### Auto-detection

Source calls `detect()` every 2 s while waiting. Detection opens test handles to all three
maps for EVO, then AC1, and immediately closes them. Only after successful detection are
persistent map handles opened. This prevents stale handles from keeping maps alive after
a game exits.

After a game disconnects (all packetIds at zero for 5 s), source drops its map handles and
re-enters the detection loop. The next game to start (same or different) is detected within
2 s and forwarding resumes with fresh compression buffers sized for that game.

### Dual maps on target

Target creates 6 named regions at startup (3 maps × 2 games), each 64 KB. Every incoming
page is written to both EVO and AC1 maps. When the source is forwarding EVO data, the EVO
maps contain live telemetry and the AC1 maps contain whatever was last written (usually
stale or zero). SimHub reads the set for its configured game; the other set is ignored.

On stale timeout (no data for `--stale-timeout` seconds), target zeroes `AC_STATUS` (offset 4
in the graphics map) in both map sets. This tells SimHub there is no active session without
destroying the maps.

### Heartbeats

When `AC_STATUS` (graphics page, offset 4) is 0 (AC_OFF — menus, loading screens), source
sends a header-only heartbeat datagram every 1 s. Target resets its stale timer on receipt
without decompressing, keeping the shared maps alive between sessions.

### Performance design

- **2 MB socket buffers** on both sides — the OS default (64 KB on Windows) is smaller than
  a single uncompressed page.
- **Pre-allocated compression and decompression buffers** — no heap allocation on the hot path.
- **Fragment reassembly** handles out-of-order datagrams; a new sequence clears prior state.
- **Receiver bounds validation** — datagram headers are checked before processing; malformed
  or spoofed packets on the LAN are silently discarded.
- **1 ms timer resolution** — source and target call `timeBeginPeriod(1)` so Windows sleep
  resolves at 1 ms granularity rather than the default 15.6 ms.
- **MMCSS on target** — registers under the Windows "Games" multimedia task for reserved CPU
  time and lower jitter. Not applied to source to avoid competing with the game's own registrations.
- **NULL DACL shared memory** — target maps are created with an explicit NULL DACL (all access),
  so SimHub and other apps can open them regardless of elevation or user account.

Release profile uses LTO, a single codegen unit, and symbol stripping.

</details>

---

## License

MIT

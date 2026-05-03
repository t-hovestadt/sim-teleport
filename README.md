# sim-bridge

Single binary for your gaming PC (source) and SimHub PC (target). Auto-detects running games, starts the right telemetry app in-process, and streams data over LAN. No manual configuration after first run.

**Supported games:** iRacing, Assetto Corsa, AC EVO, ACC, F1 series, Forza, BeamNG, PCars, Wreckfest, and more.

---

## Architecture

```
Source PC (gaming)              →    Target PC (SimHub)
  sim-bridge source                    sim-bridge target
    ├─ iRacing detected                  ├─ iRacing Teleport  :5000
    │    └─ teleport::run_source         ├─ AC Teleport        :5001
    ├─ AC/ACE/ACC detected               └─ Sim Relay        (native game ports)
    │    └─ ac_teleport::source::run
    └─ Sim Relay (always-on)
         └─ sim_relay::source::run
```

The three telemetry engines live in separate repos and are included here as git submodules:

| Submodule | Path | Purpose |
|-----------|------|---------|
| [iracing-teleport](https://github.com/t-hovestadt/iracing-teleport) | `deps/iracing-teleport` | iRacing shared-memory capture |
| [ac-teleport](https://github.com/t-hovestadt/ac-teleport) | `deps/ac-teleport` | Assetto Corsa family |
| [sim-relay](https://github.com/t-hovestadt/sim-relay) | `deps/sim-relay` | UDP relay for 35+ games |

---

## Supported games

| Game | Component | Process detected |
|------|-----------|-----------------|
| iRacing | iRacing Teleport | `iRacingSim64DX11.exe` |
| Assetto Corsa | AC Teleport | `acs.exe` |
| Assetto Corsa EVO | AC Teleport | `AssettoCorsa_EVO.exe` |
| Assetto Corsa Competizione | AC Teleport | `acc.exe` |
| F1 series, Forza, BeamNG, PCars, Wreckfest, and more | Sim Relay | built-in |

Run `sim-bridge list` for the full game list including all sim-relay UDP games.

---

## Quick start

**Gaming PC:**
```
sim-bridge.exe source
```
Or double-click `start-source.bat`. On first run, you will be prompted for IP addresses.

**SimHub PC:**
```
sim-bridge.exe target
```
Or double-click `start-target.bat`.

Both PCs must be on the same network. The gaming PC sends to the SimHub PC's IP.

**Important:** Run `sim-bridge source` only on the **gaming PC** and `sim-bridge target` only on the **SimHub PC**. Running source on both PCs causes shared memory conflicts.

**Double-click `sim-bridge.exe` directly** (no arguments): sim-bridge reads the `mode`
field from `sim-bridge.toml` and starts as source or target automatically. This is
the recommended way to use it after first-run setup — no BAT file required.

---

## Windows SmartScreen

On first run, Windows may show "Windows protected your PC." This is normal for unsigned open-source software.

To unblock: right-click the `.exe` → **Properties** → check **Unblock** at the bottom of the General tab → **OK**.

Or click **More info** on the SmartScreen dialog, then **Run anyway**.

If Windows Defender flags the file: Settings → Privacy & Security → Virus & threat protection → Manage settings → Exclusions → Add an exclusion → Folder → select the folder containing the `.exe`.

---

## Auto-start (Task Scheduler)

```
sim-bridge.exe install           # registers at logon for mode stored in config
sim-bridge.exe install --mode target  # force target mode
sim-bridge.exe uninstall         # removes the entry
```

Run as Administrator for install/uninstall.

When sim-bridge runs as a scheduled task and Windows shuts down, the process may not receive a clean shutdown signal. On next boot, SimHub may briefly show stale telemetry data until the target's stale timeout fires (default: 10 seconds). This is normal.

---

## Configuration — `sim-bridge.toml`

Created automatically on first run, next to `sim-bridge.exe`. Re-run `sim-bridge setup` to regenerate it.

| Key | Default | Description |
|-----|---------|-------------|
| `mode` | `"source"` | PC role: `"source"` (gaming) or `"target"` (SimHub) — used by `install` |
| `network.source_ip` | `192.168.50.1` | Gaming PC IP |
| `network.target_ip` | `192.168.50.2` | SimHub PC IP |
| `ports.iracing_teleport` | `5000` | iRacing Teleport port |
| `ports.ac_teleport` | `5001` | AC Teleport port |
| `detection.scan_interval` | `3` | Process scan interval in seconds |
| `detection.drain_seconds` | `20` | Grace period after game closes |
| `apps.iracing_teleport_enabled` | `true` | Enable/disable iRacing support |
| `apps.ac_teleport_enabled` | `true` | Enable/disable AC/ACE/ACC support |
| `apps.sim_relay_enabled` | `true` | Enable/disable Sim Relay |
| `apps.high_priority` | `false` | Set `HIGH_PRIORITY_CLASS` on telemetry threads |
| `apps.busy_wait` | `false` | Spin instead of sleeping (lower latency, higher CPU) |
| `apps.fanalab` | `false` | Write iRacing data to FanaLab shared memory (target only) |
| `advanced.stale_timeout_secs` | `10` | Seconds before target marks received data as stale |
| `advanced.reconnect_timeout_secs` | `5` | Seconds iRacing source waits before reset |
| `advanced.ac_poll_rate` | `60` | AC Teleport source poll rate (Hz) |
| `advanced.datagram_size` | `65000` | iRacing Teleport datagram size in bytes |

**Recommended install location:** Place `sim-bridge.exe` and `sim-bridge.toml` in a user-writable directory like `C:\Simracing\`, not in Program Files. The log file (`sim-bridge.log`) and config file are written next to the exe.

---

## Direct ethernet setup

For lowest latency, connect the two PCs with a dedicated ethernet cable (no switch).

**1. Assign static IPs**

On each PC: Network Adapter → Properties → IPv4 → Use the following IP address:

| PC | IP Address | Subnet Mask | Default Gateway |
|----|-----------|-------------|-----------------|
| Gaming (source) | `192.168.50.1` | `255.255.255.0` | *(leave blank)* |
| SimHub (target) | `192.168.50.2` | `255.255.255.0` | *(leave blank)* |

**2. Windows Firewall**

The easiest way — run `sim-bridge firewall` and paste the output into an elevated PowerShell. It reads your `sim-bridge.toml` and prints rules for both PCs.

Or manually:

**On the gaming PC** (receives resync packets from the SimHub PC):

```powershell
New-NetFirewallRule -DisplayName "sim-bridge source" `
    -Direction Inbound -Protocol UDP -LocalPort 5000,5001 -Action Allow
```

**On the SimHub PC** (receives telemetry and sim-relay game data from the gaming PC):

```powershell
New-NetFirewallRule -DisplayName "sim-bridge target" `
    -Direction Inbound -Protocol UDP `
    -LocalPort 5000,5001,5300,5606,9876,9999,15151,20777,23123,25555,30000,33740,34380,49003,63392 -Action Allow
```

**3. NIC settings (optional, for minimum latency)**

In Device Manager → Network Adapter → Properties → Advanced, set:

| Setting | Value |
|---------|-------|
| Speed & Duplex | 1 Gbps Full Duplex |
| Energy-Efficient Ethernet | Disabled |
| Power Management → Allow the computer to turn off this device | Unchecked |

**4. `sim-bridge.toml`**

```toml
mode = "source"

[network]
source_ip = "192.168.50.1"
target_ip  = "192.168.50.2"
```

---

## Console output format

sim-bridge's own log lines are timestamped: `[16:00:05] [iRacing] Detected — starting`.

Each subsystem (iRacing Teleport, AC Teleport, Sim Relay) also prints its own status
lines directly to stdout without timestamps. This is expected — the subsystem output
comes from the library crates and uses their own format. The sim-bridge timestamped
lines are the authoritative state indicator.

---

## Building from source

Requires Rust (stable) and Windows (ToolHelp32 for process scanning).

```
git clone --recurse-submodules https://github.com/t-hovestadt/sim-bridge.git
cd sim-bridge
cargo build --release
```

The binary is at `target/release/sim-bridge.exe`.

---

## Submodule management

To update all three dependencies to their latest commits:

```
git submodule update --remote
git add deps/
git commit -m "Update submodules"
```

### Versioning

sim-bridge's git submodule pointers pin the exact version of each app included in each release. To see which versions are pinned:

```
git submodule status
```

When reporting bugs, include the output of `sim-bridge --version` and `git submodule status` (if building from source).

Release workflow: update submodules, test, tag, release:

```
git submodule update --remote
cargo test
cargo build --release
git add deps/
git commit -m "Update submodules for vX.Y release"
git tag vX.Y
git push origin main --tags
```

---

## Running SimHub on the gaming PC

If you also run SimHub on the gaming PC (e.g. for a local dashboard), it reads shared memory directly from the game — sim-bridge source sends the same data to the remote SimHub PC independently. There is no conflict.

However, do **not** run `sim-bridge target` on the gaming PC. The target creates its own shared memory maps with the same names as the game's maps, which conflicts with the game.

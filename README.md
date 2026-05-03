# sim-bridge

Single binary for your gaming PC (source) and SimHub PC (target). Auto-detects running games, starts the right telemetry engine in-process, and streams data over LAN or direct ethernet.

**Supported games:** iRacing, Assetto Corsa, AC EVO, ACC, F1 series, Forza, BeamNG, PCars, Wreckfest, and more.

---

## Download

Download from the [Releases](../../releases/latest) page:

| File | Purpose |
|------|---------|
| `sim-bridge.exe` | The app — copy to both PCs |
| `start-source.bat` | Gaming PC launcher (edit to add flags) |
| `start-target.bat` | SimHub PC launcher (edit to add flags) |
| `sim-bridge.lan.toml` | Optional config template for LAN |
| `sim-bridge.direct.toml` | Optional config template for direct ethernet |

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

The three telemetry engines live in separate repos and are included as git submodules:

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

Run `sim-bridge list` for the full game list including all Sim Relay UDP games.

---

## Quick start — LAN (home router or switch)

1. Copy `sim-bridge.exe` and `start-source.bat` to the gaming PC
2. Copy `sim-bridge.exe` and `start-target.bat` to the SimHub PC
3. Double-click the bat file on each PC

No config file, no wizard, no IP addresses. iRacing and AC use multicast (`239.255.0.1`) and discover each other automatically.

**If you also play Sim Relay games** (F1, Forza, BeamNG, etc.), add `--target` to `start-source.bat`:

```batch
@echo off
cd /d "%~dp0"
sim-bridge.exe source --target 192.168.50.2
pause
```

Replace `192.168.50.2` with the SimHub PC's IP address.

---

## Quick start — Direct ethernet (lowest latency)

Dedicated cable, no router. Edit the bat files with your static IPs:

**`start-source.bat`** (gaming PC):

```batch
@echo off
cd /d "%~dp0"
sim-bridge.exe source --unicast --target 192.168.50.2 --bind 192.168.50.1
pause
```

**`start-target.bat`** (SimHub PC):

```batch
@echo off
cd /d "%~dp0"
sim-bridge.exe target --unicast --source 192.168.50.1 --high-priority --busy-wait
pause
```

**Assign static IPs** on each PC: Network Adapter → Properties → IPv4 → Use the following IP address:

| PC | IP Address | Subnet Mask | Default Gateway |
|----|-----------|-------------|-----------------|
| Gaming (source) | `192.168.50.1` | `255.255.255.0` | *(leave blank)* |
| SimHub (target) | `192.168.50.2` | `255.255.255.0` | *(leave blank)* |

**Windows Firewall:** Run `sim-bridge firewall` on either PC. It prints two labeled sections — apply each section only to the PC it describes:

```
sim-bridge.exe firewall
```

Paste the **Gaming PC** block into an elevated PowerShell on the gaming PC.  
Paste the **SimHub PC** block into an elevated PowerShell on the SimHub PC.

**NIC settings (optional, for minimum latency):** In Device Manager → Network Adapter → Properties → Advanced:

| Setting | Value |
|---------|-------|
| Speed & Duplex | 1 Gbps Full Duplex |
| Energy-Efficient Ethernet | Disabled |
| Power Management → Allow the computer to turn off this device | Unchecked |

---

## Windows SmartScreen

On first run, Windows may show "Windows protected your PC." This is normal for unsigned open-source software.

To unblock: right-click the `.exe` → **Properties** → check **Unblock** at the bottom of the General tab → **OK**.

Or click **More info** on the SmartScreen dialog, then **Run anyway**.

If Windows Defender flags the file: Settings → Privacy & Security → Virus & threat protection → Manage settings → Exclusions → Add an exclusion → Folder → select the folder containing the `.exe`.

---

## Auto-start (Task Scheduler)

```
sim-bridge.exe install                   # registers for the mode in sim-bridge.toml (or "source" if no config)
sim-bridge.exe install --mode target     # force target mode
sim-bridge.exe uninstall                 # removes the entry
```

Run as Administrator for install/uninstall. The task is registered as **SimBridge** in Task Scheduler and runs at highest privilege. To verify or remove it manually, open Task Scheduler and look for `SimBridge`.

When sim-bridge runs as a scheduled task and Windows shuts down, the process may not receive a clean shutdown signal. On next boot, SimHub may briefly show stale telemetry data until the target's stale timeout fires (default: 10 seconds). This is normal.

---

## CLI reference

```
sim-bridge.exe [SUBCOMMAND] [OPTIONS]
```

If no subcommand is given, sim-bridge reads `mode` from `sim-bridge.toml` and auto-starts as source or target (double-click friendly). If no config exists, it defaults to source mode with built-in defaults.

### `source` — gaming PC

```
sim-bridge.exe source [OPTIONS]

Options:
  --target <IP>          Target PC IP (required for Sim Relay and unicast)
  --bind <IP>            This PC's bind IP (required for unicast)
  --unicast              Direct ethernet mode (point-to-point, no multicast)
  --high-priority        Set HIGH_PRIORITY_CLASS on telemetry threads
  --busy-wait            Spin instead of sleeping (lower jitter, burns 1 core)
  --iracing-port <PORT>  iRacing Teleport port [default: 5000]
  --ac-port <PORT>       AC Teleport port [default: 5001]
  --no-iracing           Disable iRacing Teleport
  --no-ac                Disable AC Teleport
  --no-relay             Disable Sim Relay
  --scan-interval <SECS> Process scan interval [default: 3]
  --drain <SECS>         Grace period after game closes [default: 20]
```

### `target` — SimHub PC

```
sim-bridge.exe target [OPTIONS]

Options:
  --source <IP>          Source PC IP (passed to Sim Relay for filtering)
  --unicast              Direct ethernet mode (point-to-point, no multicast)
  --high-priority        Set HIGH_PRIORITY_CLASS on telemetry threads
  --busy-wait            Spin instead of sleeping (lower jitter, burns 1 core)
  --iracing-port <PORT>  iRacing Teleport port [default: 5000]
  --ac-port <PORT>       AC Teleport port [default: 5001]
  --no-iracing           Disable iRacing Teleport
  --no-ac                Disable AC Teleport
  --no-relay             Disable Sim Relay
  --fanalab              Enable FanaLab shared-memory output
```

### Other subcommands

| Subcommand | Description |
|------------|-------------|
| `setup` | Interactive wizard — writes `sim-bridge.toml`, then exits |
| `install [--mode source\|target]` | Register as a Windows logon task (run as Administrator) |
| `uninstall` | Remove the logon task (run as Administrator) |
| `list` | Print all supported games with their process names and ports |
| `firewall` | Print PowerShell `New-NetFirewallRule` commands for all configured ports |

**Note:** `sim-bridge setup` exits after writing the config. The `source` and `target` subcommands run immediately with no prompts — wizard is never triggered automatically.

**Version output** includes the pinned versions of all three engines:

```
sim-bridge --version
sim-bridge 0.1.3 (iracing-teleport 1.0.9, ac-teleport 0.2.0, sim-relay 0.1.4)
```

---

## Priority order

CLI flags override toml, toml overrides built-in defaults:

```
CLI flags  >  sim-bridge.toml  >  built-in defaults
```

This means you can have a toml file for base settings and override individual values in the bat file without editing the file.

---

## Optional: config file

For complex setups, or if you prefer config-file style over CLI flags, create `sim-bridge.toml` next to the exe. Run `sim-bridge setup` for an interactive wizard, or download a template from the Releases page.

sim-bridge looks for `sim-bridge.toml` next to the exe first, then in `%APPDATA%\sim-bridge\sim-bridge.toml`.

| Key | Default | Description |
|-----|---------|-------------|
| `mode` | `"source"` | PC role: `"source"` (gaming) or `"target"` (SimHub) — used by `install` |
| `network.unicast` | `false` | `false` = multicast LAN; `true` = unicast direct ethernet |
| `network.source_ip` | `192.168.50.1` | Gaming PC IP (used for unicast and Sim Relay) |
| `network.target_ip` | `192.168.50.2` | SimHub PC IP (used for unicast and Sim Relay) |
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
| `advanced.stale_timeout_secs` | `10` | Target: seconds before marking received data as stale |
| `advanced.reconnect_timeout_secs` | `10` | Source: iRacing reconnect timeout in seconds |
| `advanced.ac_poll_rate` | `60` | Source: AC shared-memory poll rate (Hz) |
| `advanced.datagram_size` | `9000` | Source: iRacing UDP datagram size in bytes |

**Recommended install location:** Place `sim-bridge.exe` in a user-writable directory like `C:\Simracing\`, not in Program Files. The log file (`sim-bridge.log`) and config file are written next to the exe.

---

## Console output format

sim-bridge's own log lines are timestamped: `[16:00:05] [iRacing] Detected — starting`.

Each subsystem (iRacing Teleport, AC Teleport, Sim Relay) also prints its own status lines directly to stdout without timestamps. This is expected — the subsystem output comes from the library crates and uses their own format. The sim-bridge timestamped lines are the authoritative state indicator.

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

sim-bridge's git submodule pointers pin the exact version of each engine included in each release. To see which versions are pinned:

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

# sim-bridge

Single binary for your gaming PC (source) and SimHub PC (target). Auto-detects running games, starts the right telemetry engine in-process, and streams data over LAN or direct ethernet.

**Supported games:** iRacing, Assetto Corsa, AC EVO, ACC, F1 series, Forza, BeamNG, PCars, Wreckfest, and more.

---

## Download

Download from the [Releases](../../releases/latest) page:

| File | Purpose |
|------|---------|
| `sim-bridge.exe` | The app — copy to both PCs |
| `start-source.bat` | Double-click on the gaming PC |
| `start-target.bat` | Double-click on the SimHub PC |
| `sim-bridge.lan.toml` | Config template for LAN (multicast) |
| `sim-bridge.direct.toml` | Config template for direct ethernet (unicast) |

Pick the config matching your setup, rename it to `sim-bridge.toml`, and place it next to `sim-bridge.exe` on each PC. For direct ethernet, edit the IPs. On the SimHub PC, set `mode = "target"`.

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

## LAN setup (home router or switch)

Both PCs on the same network:

1. Copy `sim-bridge.exe`, `start-source.bat`, and `start-target.bat` to a folder on each PC
2. Gaming PC: double-click `start-source.bat`
3. SimHub PC: double-click `start-target.bat`

On first run, a short wizard asks which PC role this is (source or target). iRacing and AC telemetry use multicast (`239.255.0.1`) — no IPs needed. If Sim Relay is enabled (default), the wizard also asks for the SimHub PC's IP so it can forward UDP game data.

---

## Direct ethernet setup (lowest latency)

Dedicated cable between the two PCs with no router:

**1. Run the setup wizard on each PC**

```
sim-bridge.exe setup
```

Choose `[2] Direct ethernet` and enter your static IPs.

**2. Assign static IPs**

On each PC: Network Adapter → Properties → IPv4 → Use the following IP address:

| PC | IP Address | Subnet Mask | Default Gateway |
|----|-----------|-------------|-----------------|
| Gaming (source) | `192.168.50.1` | `255.255.255.0` | *(leave blank)* |
| SimHub (target) | `192.168.50.2` | `255.255.255.0` | *(leave blank)* |

**3. Windows Firewall**

Run `sim-bridge firewall` on either PC. It prints two labeled sections — apply each section only to the PC it describes:

```
sim-bridge.exe firewall
```

Paste the **Gaming PC** block into an elevated PowerShell on the gaming PC.  
Paste the **SimHub PC** block into an elevated PowerShell on the SimHub PC.

**4. NIC settings (optional, for minimum latency)**

In Device Manager → Network Adapter → Properties → Advanced, set:

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
sim-bridge.exe install                   # registers for the mode in sim-bridge.toml
sim-bridge.exe install --mode target     # force target mode
sim-bridge.exe uninstall                 # removes the entry
```

Run as Administrator for install/uninstall. The task is registered as **SimBridge** in Task Scheduler and runs at highest privilege. To verify or remove it manually, open Task Scheduler and look for `SimBridge`.

When sim-bridge runs as a scheduled task and Windows shuts down, the process may not receive a clean shutdown signal. On next boot, SimHub may briefly show stale telemetry data until the target's stale timeout fires (default: 10 seconds). This is normal.

---

## Configuration — `sim-bridge.toml`

sim-bridge looks for `sim-bridge.toml` next to the exe first, then in `%APPDATA%\sim-bridge\sim-bridge.toml`. The file is created automatically the first time you run the setup wizard. Re-run `sim-bridge setup` to regenerate it.

| Key | Default | Description |
|-----|---------|-------------|
| `mode` | `"source"` | PC role: `"source"` (gaming) or `"target"` (SimHub) — used by `install` |
| `network.unicast` | `false` | `false` = multicast LAN (zero config); `true` = unicast direct ethernet |
| `network.source_ip` | `192.168.50.1` | Gaming PC IP (used when `unicast = true` or for Sim Relay) |
| `network.target_ip` | `192.168.50.2` | SimHub PC IP (used when `unicast = true` or for Sim Relay) |
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
| `advanced.datagram_size` | `9000` | Source: iRacing UDP datagram size in bytes (max 9000 for jumbo frames, 1472 for standard MTU) |

**Recommended install location:** Place `sim-bridge.exe` and `sim-bridge.toml` in a user-writable directory like `C:\Simracing\`, not in Program Files. The log file (`sim-bridge.log`) and config file are written next to the exe.

---

## CLI reference

```
sim-bridge.exe [SUBCOMMAND]
```

If no subcommand is given, sim-bridge reads `mode` from `sim-bridge.toml` and auto-starts as source or target (double-click friendly). If no config file exists, the setup wizard runs first and the app continues in the configured mode.

| Subcommand | Description |
|------------|-------------|
| `source` | Gaming PC: scan for running games, start the matching telemetry subsystem |
| `target` | SimHub PC: start all three telemetry receivers simultaneously |
| `setup` | Interactive wizard — writes `sim-bridge.toml`, then exits |
| `install [--mode source\|target]` | Register as a Windows logon task (run as Administrator) |
| `uninstall` | Remove the logon task (run as Administrator) |
| `list` | Print all supported games with their process names and ports |
| `firewall` | Print PowerShell `New-NetFirewallRule` commands for all configured ports |

Note: `sim-bridge setup` exits after writing the config. `sim-bridge source` and `sim-bridge target` run the wizard on first use and then continue running.

**Version output** includes the pinned versions of all three telemetry engines:

```
sim-bridge --version
sim-bridge 0.1.3 (iracing-teleport 1.0.9, ac-teleport 0.2.0, sim-relay 0.1.4)
```

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

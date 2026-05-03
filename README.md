# sim-bridge

Unified telemetry bridge for sim racing. One binary handles iRacing,
Assetto Corsa (EVO, AC1, ACC), and 35+ UDP games (F1, Forza, BeamNG,
Project Cars, Wreckfest, and more). Runs on both the gaming PC and
SimHub PC.

For iRacing and Assetto Corsa on a LAN, it works with zero configuration.
For direct ethernet or UDP relay games, add your IPs to the bat file.

---

## Download

Download from the [Releases](../../releases/latest) page:

| File | Description |
|------|-------------|
| `sim-bridge.exe` | Copy to both PCs |
| `start-source.bat` | Example bat for the gaming PC |
| `start-target.bat` | Example bat for the SimHub PC |

---

## Windows SmartScreen

On first run, Windows may show "Windows protected your PC." This is normal for unsigned open-source software.

To unblock: right-click the `.exe` → **Properties** → check **Unblock** at the bottom of the General tab → **OK**.

Or click **More info** on the SmartScreen dialog, then **Run anyway**.

---

## LAN setup (zero config for iRacing and AC)

**Gaming PC** (`start-source.bat`):
```batch
@echo off
cd /d "%~dp0"
sim-bridge.exe source
pause
```

**SimHub PC** (`start-target.bat`):
```batch
@echo off
cd /d "%~dp0"
sim-bridge.exe target
pause
```

Copy `sim-bridge.exe` and the appropriate bat file to each PC. Double-click to run.
iRacing and Assetto Corsa (EVO, AC1, ACC) work immediately via multicast — no IPs needed.

To also forward UDP relay games (F1, Forza, BeamNG, etc.), add the SimHub
PC's IP so sim-relay knows where to send:

```batch
sim-bridge.exe source --target 192.168.50.2
```

---

## Direct ethernet setup (lowest latency)

Connect both PCs with a dedicated ethernet cable. Assign static IPs:

| PC | IP | Subnet | Gateway |
|----|----|--------|---------|
| Gaming PC | `192.168.50.1` | `255.255.255.0` | *(leave blank)* |
| SimHub PC | `192.168.50.2` | `255.255.255.0` | *(leave blank)* |

In Windows: *Network & Internet → Change adapter options → right-click adapter → Properties → IPv4 → Use the following IP address*.

**Gaming PC** (`start-source.bat`):
```batch
@echo off
cd /d "%~dp0"
sim-bridge.exe source --unicast --target 192.168.50.2 --bind 192.168.50.1
pause
```

**SimHub PC** (`start-target.bat`):
```batch
@echo off
cd /d "%~dp0"
sim-bridge.exe target --unicast --source 192.168.50.1 --high-priority --busy-wait
pause
```

### Firewall rules

Run `sim-bridge firewall` for copy-paste PowerShell commands, or run these manually as Administrator:

**Gaming PC** (receives resync packets from SimHub PC):
```powershell
New-NetFirewallRule -DisplayName "sim-bridge source" `
    -Direction Inbound -Protocol UDP -LocalPort 5000,5001 -Action Allow
```

**SimHub PC** (receives telemetry and game data from gaming PC):
```powershell
New-NetFirewallRule -DisplayName "sim-bridge target" `
    -Direction Inbound -Protocol UDP `
    -LocalPort 5000,5001,5300,5606,9876,9999,15151,20777,23123,25555,30000,33740,34380,49003,63392 `
    -Action Allow
```

### NIC settings (optional, for minimum latency)

In Device Manager → Network Adapter → Properties → Advanced:

| Setting | Value |
|---------|-------|
| Speed & Duplex | 1 Gbps Full Duplex |
| Energy-Efficient Ethernet | Disabled |
| Interrupt Moderation / Interrupt Throttle Rate | Disabled |
| Wake on Magic Packet | Disabled |
| Wake on Pattern Match | Disabled |
| Auto MDI/MDIX | Auto |

Power Management tab: uncheck **"Allow the computer to turn off this device to save power"** and **"Allow this device to wake the computer"** on both PCs.

Setting names vary by NIC manufacturer — look for equivalents if the exact names differ.

### Troubleshooting

*Adapter shows Disconnected despite cable plugged in:* Do a full **Shut down** (not Restart), wait 30–60 seconds, then power on. Disable Wake-on-LAN in both the NIC settings above and BIOS ("Wake on LAN" / "PCIe ASPM").

*Link won't establish between two NICs:* Set Speed & Duplex to 1.0 Gbps Full Duplex and confirm **Auto MDI/MDIX** is Auto — if disabled, a straight-through cable won't link without a crossover cable.

*Can't set static IP via PowerShell (`element not found`):* Plug the cable in first so the adapter shows a link, then set the IP. To reset: `Remove-NetIPAddress -InterfaceIndex <N> -Confirm:$false` then re-add.

---

## Architecture

```
Source PC (gaming)                Target PC (SimHub)
  sim-bridge source                sim-bridge target
    |- iRacing detected              |- iRacing Teleport  :5000
    |    \- teleport source          |- AC Teleport       :5001
    |- AC/EVO/ACC detected           \- Sim Relay         (game ports)
    |    \- ac-teleport source
    \- Sim Relay (always on)
         \- sim-relay source
```

Three telemetry engines in one binary. On the source, shared-memory apps
(iRacing, AC) start and stop based on which game is running. Sim Relay runs
continuously and handles its own game detection for UDP titles.

On the target, all three receivers run simultaneously. Each blocks on
`recv()` and costs zero CPU when idle. Crashed threads restart automatically
with exponential backoff.

---

## Supported games

**Shared memory (auto-detected by process name, started by sim-bridge):**

| Game | Process |
|------|---------|
| iRacing | `iRacingSim64DX11.exe` |
| Assetto Corsa EVO | `AssettoCorsa_EVO.exe` |
| Assetto Corsa | `acs.exe` |
| Assetto Corsa Competizione | `acc.exe` |

Only one shared-memory game runs at a time on the source. If you close one
and open another, sim-bridge switches automatically within one scan interval (default 3 s).

**UDP relay (auto-detected by sim-relay, always running on source):**

Run `sim-bridge list` for the full list of 35+ supported titles including
F1 25, Forza Motorsport, Forza Horizon 5, Project Cars 2, Automobilista 2,
BeamNG, Wreckfest 2, DiRT Rally 2.0, Euro/American Truck Simulator, and more.

---

## CLI reference

### `sim-bridge source [OPTIONS]`

| Flag | Description |
|------|-------------|
| `--target <IP>` | SimHub PC's IP address. Required for sim-relay forwarding; also used for iRacing/AC in unicast mode. |
| `--bind <IP>` | This PC's IP address. Required for unicast mode (binds the socket so resync packets return correctly). |
| `--unicast` | Direct ethernet mode — send/receive without multicast. Use for a direct cable connection. |
| `--high-priority` | Set `HIGH_PRIORITY_CLASS` on telemetry threads. |
| `--busy-wait` | Spin-wait instead of sleeping (lower jitter, burns one CPU core). |
| `--iracing-port <PORT>` | iRacing Teleport port (default: 5000). |
| `--ac-port <PORT>` | AC Teleport port (default: 5001). |
| `--no-iracing` | Disable iRacing Teleport. |
| `--no-ac` | Disable AC Teleport. |
| `--no-relay` | Disable Sim Relay. |
| `--scan-interval <SECS>` | How often to scan for running game processes (default: 3 s). |
| `--drain <SECS>` | Grace period to keep forwarding after a game closes (default: 20 s). |

### `sim-bridge target [OPTIONS]`

| Flag | Description |
|------|-------------|
| `--source <IP>` | Gaming PC's IP address. Passed to Sim Relay for packet filtering. |
| `--unicast` | Direct ethernet mode — receive without joining a multicast group. |
| `--high-priority` | Set `HIGH_PRIORITY_CLASS` on telemetry threads. |
| `--busy-wait` | Spin-wait instead of sleeping (lower jitter, burns one CPU core). |
| `--iracing-port <PORT>` | iRacing Teleport port (default: 5000). |
| `--ac-port <PORT>` | AC Teleport port (default: 5001). |
| `--no-iracing` | Disable iRacing Teleport. |
| `--no-ac` | Disable AC Teleport. |
| `--no-relay` | Disable Sim Relay. |
| `--fanalab` | Write iRacing data to FanaLab shared memory (target only). |

### Other commands

| Command | Description |
|---------|-------------|
| `sim-bridge setup` | Interactive config wizard (creates `sim-bridge.toml`). |
| `sim-bridge install [--mode source\|target]` | Register auto-start at Windows logon (Task Scheduler). |
| `sim-bridge uninstall` | Remove auto-start. |
| `sim-bridge list` | Show all supported games. |
| `sim-bridge firewall` | Print copy-paste firewall rules for both PCs. |
| `sim-bridge --version` | Show version including sub-app versions. |

---

## Auto-start (Task Scheduler)

```
sim-bridge install                    # registers using mode from sim-bridge.toml, or "source"
sim-bridge install --mode target      # force target mode
sim-bridge uninstall
```

Run as Administrator. The registered command is `sim-bridge.exe source` or
`sim-bridge.exe target` with no CLI flags — settings come from `sim-bridge.toml`
if present. For persistent IPs or port overrides via auto-start, use a config file.

On reboot, sim-bridge starts automatically. SimHub may briefly show stale
telemetry (up to 10 seconds) until the target's stale timeout clears old data.
This is normal when the process doesn't receive a clean shutdown signal.

---

## Optional: config file

For persistent settings without repeating CLI flags every run, create
`sim-bridge.toml` next to `sim-bridge.exe`. CLI flags always override the
config file.

Run `sim-bridge setup` for a guided wizard, or create the file manually:

```toml
# sim-bridge.toml — example for direct ethernet

[network]
unicast    = true
source_ip  = "192.168.50.1"   # gaming PC
target_ip  = "192.168.50.2"   # SimHub PC

[apps]
high_priority = true
busy_wait     = true
```

`sim-bridge.toml` is also read by `sim-bridge install` to determine the
default mode to register.

### Config fields

| Field | Default | Description |
|-------|---------|-------------|
| `mode` | `"source"` | PC role: `"source"` (gaming) or `"target"` (SimHub). Used by `install`. |
| `network.source_ip` | `"192.168.50.1"` | Gaming PC IP. Bind address in unicast mode (source); sim-relay source filter (target). |
| `network.target_ip` | `"192.168.50.2"` | SimHub PC IP. Required for sim-relay forwarding; also for iRacing/AC in unicast mode. |
| `network.unicast` | `false` | `true` = direct ethernet (no multicast). `false` = LAN (multicast, no IP config needed for iRacing/AC). |
| `ports.iracing_teleport` | `5000` | iRacing Teleport UDP port. |
| `ports.ac_teleport` | `5001` | AC Teleport UDP port. |
| `detection.scan_interval` | `3` | Process scan interval in seconds (source only). |
| `detection.drain_seconds` | `20` | Grace period after game closes before stopping the telemetry thread. |
| `apps.iracing_teleport_enabled` | `true` | Set `false` to disable iRacing Teleport entirely. |
| `apps.ac_teleport_enabled` | `true` | Set `false` to disable AC Teleport entirely. |
| `apps.sim_relay_enabled` | `true` | Set `false` to disable Sim Relay entirely. |
| `apps.high_priority` | `false` | Set `HIGH_PRIORITY_CLASS` on telemetry threads. |
| `apps.busy_wait` | `false` | Spin instead of sleeping (lower latency, higher CPU). |
| `apps.fanalab` | `false` | Write iRacing data to FanaLab shared memory (target only). |
| `advanced.stale_timeout_secs` | `10` | Seconds without data before target marks telemetry as stale. |
| `advanced.reconnect_timeout_secs` | `10` | Seconds iRacing source waits for data before reconnecting. |
| `advanced.ac_poll_rate` | `60` | AC Teleport source poll rate (Hz). |
| `advanced.datagram_size` | `9000` | iRacing Teleport UDP datagram size in bytes. |

The config file is looked up next to `sim-bridge.exe` first, then at
`%APPDATA%\sim-bridge\sim-bridge.toml`. If neither exists, built-in defaults
apply and CLI flags alone control all behaviour.

---

## Running SimHub on the gaming PC

If you also run SimHub on the gaming PC (e.g. for a local dashboard), it reads
shared memory directly from the game. sim-bridge source sends the same data to
the remote SimHub PC independently — there is no conflict.

**Do not** run `sim-bridge target` on the gaming PC. The target creates its own
shared memory maps with the same names as the game, which conflicts with the game.

---

## Building from source

Requires Rust (stable). Windows is required for process scanning (ToolHelp32 API).

```
git clone --recurse-submodules https://github.com/t-hovestadt/sim-bridge.git
cd sim-bridge
cargo build --release
```

The binary is at `target/release/sim-bridge.exe`.

---

## Companion projects

| Repo | Purpose |
|------|---------|
| [iracing-teleport](https://github.com/t-hovestadt/iracing-teleport) | iRacing shared-memory streaming (standalone) |
| [ac-teleport](https://github.com/t-hovestadt/ac-teleport) | Assetto Corsa shared-memory streaming (standalone) |
| [sim-relay](https://github.com/t-hovestadt/sim-relay) | UDP relay for 35+ games (standalone) |

Each app works independently. sim-bridge bundles all three with unified
game detection.

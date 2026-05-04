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

sim-relay traffic arrives on `game_port + 10000` (the default port offset) to avoid binding
conflicts with SimHub. Run `sim-bridge firewall` to generate the exact rule for your config.

```powershell
New-NetFirewallRule -DisplayName "sim-bridge target" `
    -Direction Inbound -Protocol UDP `
    -LocalPort 5000,5001,15300,15606,19876,19999,25151,30777,33123,35555,40000,43740,44380,59003 `
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
    \- UDP game detected
         \- sim-relay source
```

Three telemetry engines in one binary. On the source, **only one game streams
telemetry at a time**. sim-bridge detects all running games — shared-memory
(iRacing, AC variants) and UDP relay titles — and enforces priority:
iRacing > AC variants > UDP relay games. When the active game exits, the next
highest-priority detected game takes over after a 20-second drain period.

On the target, all three receivers run simultaneously. Each blocks on
`recv()` and costs zero CPU when idle. Crashed threads restart automatically
with exponential backoff.

---

## Supported games

**Shared memory (auto-detected, started by sim-bridge):**

| Game | Detection method |
|------|-----------------|
| iRacing | Process name — `iRacingSim64DX11.exe` in running process list |
| Assetto Corsa EVO | Shared-memory probe — `packetId` liveness check on `acevo_pmf_physics` |
| Assetto Corsa | Shared-memory probe — `packetId` liveness check on `acpmf_physics` |
| Assetto Corsa Competizione | Shared-memory probe — same as AC1, with `acc.exe` process tiebreaker |

Only one shared-memory game runs at a time on the source. If you close one
and open another, sim-bridge switches automatically within one scan interval (default 3 s).

**UDP relay (auto-detected by sim-bridge, started on demand):**

Run `sim-bridge list` for the full list of 35+ supported titles including
F1 25, Forza Motorsport, Forza Horizon 5, Project Cars 2, Automobilista 2,
BeamNG, Wreckfest 2, DiRT Rally 2.0, Euro/American Truck Simulator, and more.

---

## Per-game setup notes

Most UDP relay games work automatically once the process is detected. A few
require in-game settings or config files before they send telemetry.

### Wreckfest 2

Wreckfest 2 does **not** send telemetry by default. You must create a config
file manually.

1. Find your profile folder:
   ```
   %USERPROFILE%\Documents\My Games\Wreckfest 2\
   ```
   Inside you'll find a numbered folder (your Steam ID). Open it.

2. Create the path `savegame\telemetry\` if it doesn't exist.

3. Create `config.json` in that folder with this content:
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

4. Full path example:
   ```
   %USERPROFILE%\Documents\My Games\Wreckfest 2\76561198012345678\savegame\telemetry\config.json
   ```

5. Restart Wreckfest 2. sim-bridge will detect the process and start relaying
   telemetry automatically.

### F1 25 / F1 24 / DiRT Rally 2.0 / WRC

Enable UDP telemetry in **Game Options → Settings → Telemetry Settings**. Set
port to `20777` and IP to `127.0.0.1`.

### Euro / American Truck Simulator

Install the [SCS SDK Telemetry Plugin](https://github.com/RenCloud/scs-sdk-plugin).
The plugin creates a shared memory interface that sim-relay forwards as UDP.

---

## SimHub setup on the target PC

SimHub detects games via shared memory maps and UDP packets. On the target PC,
no game process runs — sim-bridge creates the maps and forwards the UDP.
sim-bridge also tells SimHub which game is active via the `-switchgame` command.

### One-time setup per game

**iRacing** — works automatically. SimHub reads the shared memory maps created
by sim-bridge target.

**Assetto Corsa (AC1 / ACC / EVO)** — SimHub may show "game not configured" or
fail to activate AC on first launch. Fix:

1. Open SimHub on the target PC
2. Left sidebar → find **Assetto Corsa** in the game list
3. If it shows a warning: right-click → **Enable** (or click through any
   configuration wizard, choosing "Configure manually" to skip the game-path scan)
4. SimHub will now read `acpmf_physics`, `acpmf_graphics`, `acpmf_static` maps
   when sim-bridge switches to AC

Repeat for **Assetto Corsa Competizione** if you also use ACC.
For **Assetto Corsa EVO**, check whether your SimHub version includes an EVO
entry — it may need a SimHub update.

**UDP games (F1, Forza, BeamNG, Wreckfest 2, etc.)** — work automatically.
SimHub listens on each game's UDP port and identifies the packet format.

### SimHub in-game app (opponent tracking)

SimHub's Assetto Corsa in-game app creates a separate shared memory map
(`acpmf_simhub_v2`) on the gaming PC for opponent tracking and leaderboard data.
This map is **not** forwarded by sim-bridge — only the three core telemetry maps
(`acpmf_physics`, `acpmf_graphics`, `acpmf_static`) are forwarded.

**What works via sim-bridge:** speed, RPM, gear, throttle/brake, temperatures,
tyre data, lap times, position, session info.

**What does not work:** opponent tracking, leaderboard overlays that read from
`acpmf_simhub_v2`. These require the SimHub app running inside AC on the gaming PC
and the map being forwarded — currently out of scope.

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
| `--scan-interval <SECS>` | How often to run the game detection cycle (default: 3 s). |
| `--drain <SECS>` | Grace period to keep forwarding after a game closes (default: 20 s). |
| `--verbose` | Print detailed detection results each scan cycle (probe outcomes, process matches). |

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

### Session report

sim-bridge writes `sim-bridge-report.txt` next to the exe every 60 seconds and on
clean shutdown. It contains detection counters (total scan cycles, probe counts,
process matches), a session history (which game started, how it was detected, when
it stopped and why), and any errors logged during the run. Open it in any text
editor for a quick post-session diagnostic summary.

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
| `detection.scan_interval` | `3` | Detection cycle interval in seconds (source only). |
| `detection.drain_seconds` | `20` | Grace period after game closes before stopping the telemetry thread. |
| `apps.iracing_teleport_enabled` | `true` | Set `false` to disable iRacing Teleport entirely. |
| `apps.ac_teleport_enabled` | `true` | Set `false` to disable AC Teleport entirely. |
| `apps.sim_relay_enabled` | `true` | Set `false` to disable Sim Relay entirely. |
| `apps.high_priority` | `false` | Set `HIGH_PRIORITY_CLASS` on telemetry threads. |
| `apps.busy_wait` | `false` | Spin instead of sleeping (lower latency, higher CPU). |
| `apps.fanalab` | `false` | Write iRacing data to FanaLab shared memory (target only). |
| `apps.relay_port_offset` | `10000` | Port offset for Sim Relay. Target listens on `game_port + offset`; source sends to `target:(game_port + offset)`; SimHub reads the original `game_port`. Avoids binding conflict between sim-relay and SimHub on the target PC. |
| `advanced.stale_timeout_secs` | `10` | Seconds without data before target marks telemetry as stale. |
| `advanced.reconnect_timeout_secs` | `10` | Seconds iRacing source waits for data before reconnecting. |
| `advanced.ac_poll_rate` | `60` | AC Teleport source poll rate (Hz). |
| `advanced.datagram_size` | `9000` | iRacing Teleport UDP datagram size in bytes. |
| `simhub.path` | *(default install)* | Path to `SimHubWPF.exe`. Defaults to `C:\Program Files (x86)\SimHub\SimHubWPF.exe`. |
| `simhub.iracing` | `"iRacing"` | SimHub game code passed to `-switchgame` when iRacing telemetry starts. |
| `simhub.ac` | `"AssettoCorsa"` | SimHub game code passed to `-switchgame` when AC/EVO/ACC telemetry starts. |
| `simhub.relay.<id>` | *(none)* | SimHub game code for a sim-relay game. Key is the sim-relay game ID (e.g. `wreckfest2`). |

The `[simhub]` section is optional. When configured (or when `SimHubWPF.exe` exists at the
default path), sim-bridge runs `SimHubWPF.exe -switchgame <code>` once when telemetry from a
new game is first received on the target PC. Resets automatically when the stale timeout fires
so the next session re-triggers the switch.

Most sim-relay games have built-in SimHub code mappings (Wreckfest 2, F1 2018–25, DiRT Rally 2.0,
BeamNG, WRC, Project CARS 2, AMS2, Forza) — no config needed for those. Use `[simhub.relay]`
only to override a built-in code or to add support for a game not in the built-in list:

```toml
[simhub.relay]
my-custom-game = "SimHubGameCode"
```

SimHub game codes are the internal names SimHub uses for each title. If unsure, check SimHub's
game list or the SimHub forum for the correct code string.

The config file is looked up next to `sim-bridge.exe` first, then at
`%APPDATA%\sim-bridge\sim-bridge.toml`. If neither exists, built-in defaults
apply and CLI flags alone control all behaviour.

---

## Troubleshooting: SimHub not showing telemetry

### Assetto Corsa / ACE / ACC

On the **first run** of `sim-bridge target`, it writes `GameSettings.json` entries so SimHub
skips the "game not configured" prompt for AC, ACE, ACC, iRacing, and Wreckfest 2. It also
creates the per-game PluginsData subfolder. If changes were made, the log prints:

```
[SimHub] Configured AssettoCorsa in GameSettings.json
[SimHub] *** Configuration updated. Please restart SimHub to apply changes. ***
```

Restart SimHub after this message. Subsequent runs are silent (idempotent).

If SimHub still shows no AC telemetry after restarting:

1. Open SimHub on the target PC.
2. Go to **Settings** → **In-game apps** tab.
3. Find **Assetto Corsa** in the list — enable it if not already enabled.
4. Restart SimHub.

The target log also prints a reminder when the first AC frame arrives.

### Wreckfest 2 and other sim-relay games

Verify data is flowing: the target log should show:

```
[Wreckfest 2] traffic received → 127.0.0.1:23123
```

If that line is present, telemetry is arriving at the correct port and SimHub should read it.
If SimHub still shows nothing:

- SimHub 2025 and later include Wreckfest 2 support. Earlier versions may not recognise the
  packet format — update SimHub if needed.
- sim-bridge has a built-in Wreckfest 2 → "Wreckfest2" code; no `[simhub.relay]` config needed.

### General checklist

| Symptom | Likely cause |
|---------|-------------|
| Target log shows 0 msg/s | Data not reaching target — check source log and network |
| Target log shows msg/s but SimHub blank | Restart SimHub after first run (auto-config applied) |
| `[simhub] switched to ...` missing | `simhub.path` points to wrong location, or SimHubWPF.exe not found |
| SimHub shows wrong game overlay | Game not in built-in code table; add to `[simhub.relay]` |
| `[SimHub] Configuration updated` on every run | SimHub restoring defaults; check SimHub version |

---

## Running SimHub on the gaming PC

If you also run SimHub on the gaming PC (e.g. for a local dashboard), it reads
shared memory directly from the game. sim-bridge source sends the same data to
the remote SimHub PC independently — there is no conflict.

**Do not** run `sim-bridge target` on the gaming PC. The target creates its own
shared memory maps with the same names as the game, which conflicts with the game.

---

## Building from source

Requires Rust (stable). Windows is required for the APIs used for game detection (Named Events, shared-memory sections, ToolHelp32).

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

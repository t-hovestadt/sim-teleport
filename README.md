# sim-teleport

Unified telemetry bridge for sim racing. One binary handles iRacing,
Assetto Corsa (EVO, AC1, ACC), and 35+ UDP games (F1, Forza, BeamNG,
Wreckfest 2, and more). Runs on both the gaming PC and SimHub PC.

iRacing and Assetto Corsa work on LAN with zero configuration. Direct
ethernet or UDP relay games need two IPs in the bat file.

```
  Gaming PC (sim-teleport source)               SimHub PC (sim-teleport target)

  ┌────────────────────────┐                  ┌────────────────────────────┐
  │  iRacing               │                  │  iRacing Teleport  :5000   │
  │  AC EVO / AC1 / ACC    │  ─── UDP ──────► │  AC Teleport       :5001   │
  │  Wreckfest 2 / F1 / …  │                  │  Sim Relay  (game ports)   │
  └────────────────────────┘                  └────────────────────────────┘
                                                          │
                                                          ▼
                                                  SimHub reads shared memory
                                                  and UDP packets as if the
                                                  game were running locally
```

---

## Download

Download from the [Releases](../../releases/latest) page:

| File | Description |
|------|-------------|
| `sim-teleport.exe` | Copy to both PCs |
| `start-source.bat` | Gaming PC — edit IPs, double-click to run |
| `start-target.bat` | SimHub PC — double-click to run |
| `sim-teleport.lan.toml` | Config template for LAN (multicast) setup |
| `sim-teleport.direct.toml` | Config template for direct ethernet |

---

## Windows SmartScreen

On first run, Windows may show "Windows protected your PC." This is
normal for unsigned open-source software.

To unblock: right-click the `.exe` → **Properties** → check **Unblock** at the
bottom of the General tab → **OK**.

Or click **More info** on the SmartScreen dialog, then **Run anyway**.

---

## LAN setup (zero config for iRacing and AC)

**Gaming PC** (`start-source.bat`):
```batch
@echo off
cd /d "%~dp0"
sim-teleport.exe source
pause
```

**SimHub PC** (`start-target.bat`):
```batch
@echo off
cd /d "%~dp0"
sim-teleport.exe target
pause
```

Copy `sim-teleport.exe` and the appropriate bat file to each PC. Double-click
to run. iRacing and Assetto Corsa (EVO, AC1, ACC) work via multicast — no
IPs needed.

To also forward UDP relay games (F1, Forza, BeamNG, etc.), add your SimHub
PC's IP so sim-relay knows where to send:

```batch
sim-teleport.exe source --target 192.168.50.2
```

---

## Direct ethernet setup (lowest latency)

Connect both PCs with a dedicated ethernet cable. Assign static IPs:

| PC | IP | Subnet | Gateway |
|----|----|--------|---------|
| Gaming PC | `192.168.50.1` | `255.255.255.0` | *(leave blank)* |
| SimHub PC | `192.168.50.2` | `255.255.255.0` | *(leave blank)* |

In Windows: *Network & Internet → Change adapter options → right-click
adapter → Properties → IPv4 → Use the following IP address*.

**Gaming PC** (`start-source.bat`):
```batch
@echo off
cd /d "%~dp0"
sim-teleport.exe source --unicast --target 192.168.50.2 --bind 192.168.50.1
pause
```

**SimHub PC** (`start-target.bat`):
```batch
@echo off
cd /d "%~dp0"
sim-teleport.exe target --unicast --source 192.168.50.1 --high-priority --busy-wait
pause
```

`--high-priority --busy-wait` on the target reduce scheduling jitter. They
are safe on the SimHub PC because the game is not running there. Do not
use them on the gaming PC — they compete with the game.

### Firewall rules

Run `sim-teleport firewall` for copy-paste PowerShell commands. Manually:

**Gaming PC** (receives iRacing/AC resync packets):
```powershell
New-NetFirewallRule -DisplayName "sim-teleport source" `
    -Direction Inbound -Protocol UDP -LocalPort 5000,5001 -Action Allow
```

**SimHub PC** (receives telemetry + relay data). sim-relay traffic arrives
on `game_port + 10000` (default offset) to avoid binding conflicts with SimHub:
```powershell
New-NetFirewallRule -DisplayName "sim-teleport target" `
    -Direction Inbound -Protocol UDP `
    -LocalPort 5000,5001,15300,15606,19876,19999,25151,30777,33123,35555,40000,43740,44380,59003 `
    -Action Allow
```

### NIC settings (optional, for minimum latency)

Device Manager → Network Adapter → Properties → Advanced:

| Setting | Value |
|---------|-------|
| Speed & Duplex | 1 Gbps Full Duplex |
| Energy-Efficient Ethernet | Disabled |
| Interrupt Moderation / Interrupt Throttle Rate | Disabled |
| Wake on Magic Packet | Disabled |
| Wake on Pattern Match | Disabled |
| Auto MDI/MDIX | Auto |

Power Management tab: uncheck "Allow the computer to turn off this device"
and "Allow this device to wake the computer" on both PCs.

**Troubleshooting link problems**

*Adapter shows Disconnected despite cable plugged in:* Do a full Shut down
(not Restart), wait 30–60 seconds, then power on. Disable Wake-on-LAN in
the NIC settings above and in BIOS.

*Link won't establish:* Set Speed & Duplex to 1.0 Gbps Full Duplex and
confirm Auto MDI/MDIX is Auto — if disabled, a straight-through cable
won't link without a crossover cable.

*Can't set static IP via PowerShell (`element not found`):* Plug the cable
in first so the adapter shows a link, then set the IP. To reset:
`Remove-NetIPAddress -InterfaceIndex <N> -Confirm:$false`.

---

## Architecture

### What runs where

**Source PC** runs one game at a time. sim-teleport detects which game is
active (by probing shared memory or scanning running processes) and forwards
only that game's telemetry to the target. Priority order when multiple games
are detected: iRacing > AC variants > UDP relay games.

**Target PC** runs all three receivers simultaneously — iRacing Teleport
(:5000), AC Teleport (:5001), and Sim Relay (game ports). Each blocks on
`recv()` at zero CPU when idle. The correct receiver gets data automatically
without any game switching on the target.

### Thread model

```
main thread
 ├─ scanner loop  (every 3s by default)
 │    ├─ probe iRacing shmem
 │    ├─ probe AC/EVO/ACC shmem
 │    └─ scan process list (sim-relay games)
 │
 ├─ iRacing Teleport thread  (blocks on IRSDKDataValidEvent / stale timer)
 ├─ AC Teleport thread       (blocks on UDP recv / stale timer)
 └─ Sim Relay thread         (blocks on UDP recv / stale timer)
```

Crashed threads restart automatically with exponential backoff:
2 s → 5 s → 15 s → 60 s.

### Repository layout

sim-teleport is a Cargo workspace. iracing-teleport ships as a git submodule;
ac-teleport and sim-relay live directly in the repo under `crates/`:

```
sim-teleport/
├── src/
│   ├── source/
│   │   ├── mod.rs        ← shared types (ShmemGame, Detection) + run()
│   │   ├── detection.rs  ← game detection cycle and liveness checks
│   │   ├── slot.rs       ← AppSlot lifecycle (start/drain/stop)
│   │   └── wreckfest.rs  ← Wreckfest 2 telemetry config creation
│   └── target/  ...
├── crates/
│   ├── ac-teleport/     ← workspace crate (AC1 / AC EVO / ACC)
│   └── sim-relay/       ← workspace crate (35+ UDP games)
└── deps/
    └── iracing-teleport/ ← git submodule
```

Each crate compiles as a library. sim-teleport calls their `run_source()` /
`run_target()` functions, passing a `shutdown: Receiver<()>` channel and
callback closures (`on_first_data`, `on_stale`, `on_game_announce`).
The callbacks wire each receiver to `ActiveGameTracker` and `StubManager`
without the sub-crates knowing anything about SimHub or Windows registry.

---

## Game detection

### Source detection cycle

Every 3 seconds (configurable with `--scan-interval`):

1. If iRacing is enabled: look for `iRacingSim64DX11.exe` in the running
   process list (via Windows ToolHelp32 snapshot).
2. If AC is enabled: probe shared-memory map names in priority order:
   EVO → AC1 → ACC (see below).
3. If sim-relay is enabled: scan the process list for all registered game
   executables.

When a game is detected, the corresponding telemetry thread (which is
always running) receives a start signal and begins forwarding. Only one
game runs at a time on the source.

### Shared-memory probing (AC games)

Probing works by:
1. `OpenFileMappingW(FILE_MAP_READ, 0, map_name)` — try to open the map
2. If it opens: `MapViewOfFile` → read `packetId` (i32 at byte offset 0)
3. Sleep 100 ms → read `packetId` again
4. Unmap and close handle

If `packetId` changed between reads → **Live** (game running and in session).
If `packetId` unchanged → **Stale** (map is a ghost from a previous session).

**Ghost maps**: Windows doesn't clean up named shared-memory regions when
a game exits — the region stays alive until the last open handle is closed.
If SimHub has the map open, the region persists indefinitely after the game
quits. Stale `packetId` (no change in 100 ms) is how we detect this case.
A process-name tiebreaker confirms: if `acs.exe` (or `acc.exe`,
`AssettoCorsa_EVO.exe`) is not in the running process list, the map is
a ghost and is ignored.

**EVO vs AC1 disambiguation**: EVO uses `Local\acevo_pmf_physics`, AC1 and
ACC both use `Local\acpmf_physics`. EVO is probed first. If EVO is live,
AC1/ACC are skipped. If `Local\acpmf_physics` is live, the tiebreaker
checks for `acc.exe` vs `acs.exe` to distinguish ACC from AC1.

### 3-scan liveness rule

A game must be missing from detection for **3 consecutive scan cycles**
(default: 9 seconds) before sim-teleport sends a shutdown signal to the
telemetry thread. This prevents rapid start/stop loops from process
flickering during AC session transitions (the AC launcher spawns and kills
several child processes when loading a session — transient absences of
`acs.exe` during this window would otherwise cause premature shutdown and
reconnect churn).

### Why iRacing uses process detection instead of shared memory

Early versions probed the `IRSDKDataValidEvent` named event. This was
reverted because FanaLab holds the event handle open after iRacing exits,
which made iRacing appear live long after it had quit. Process name
detection (`iRacingSim64DX11.exe` in the ToolHelp32 snapshot) is immune
to this — when iRacing exits the process disappears.

### Detection state machine

```
    Idle ──(detected)──► Running ──(not detected × 3)──► Draining ──(20s)──► Idle
                                       │                       │
                                  (still detected)        (detected again)
                                       │                       │
                                       └───────────────────────┘
                                          (stay / resume Running)
```

The 20-second drain period keeps telemetry flowing after the game closes,
giving SimHub time to receive the final frame before the source goes silent.

---

## Supported games

**Shared memory (auto-detected):**

| Game | Detection | Telemetry | SimHub switch | Notes |
|------|-----------|-----------|---------------|-------|
| iRacing | `iRacingSim64DX11.exe` process | 60 Hz mirroring | `iRacing` | Fully working |
| Assetto Corsa EVO | `acevo_pmf_physics` shmem probe | 60 Hz raw bytes | `AssettoCorsaEVO` | Working; requires EVO installed on target or fake install |
| Assetto Corsa | `acpmf_physics` shmem probe | 120 Hz raw bytes | `AssettoCorsa` | Working |
| Assetto Corsa Competizione | `acpmf_physics` + `acc.exe` tiebreaker | shmem mirroring | `AssettoCorsaCompetizione` | Supported; primary interface is UDP port 9000 |

**UDP relay (35+ games via sim-relay):**

Run `sim-teleport list` for the full table. Key entries:

| Game | Port | SimHub code |
|------|------|-------------|
| Wreckfest 2 | 23123 | `Wreckfest2` |
| F1 25 / F1 24 / … / F1 2018 | 20777 | `F12025` / `F12024` / … |
| DiRT Rally 2.0 / DiRT 5 | 20777 | `DirtRally2` / `Dirt5` |
| WRC 2023 / 2024 | 20777 | `WRC2023` / `WRC2024` |
| Forza Motorsport (2023) | 9876 | `ForzaMotorsport` |
| Forza Horizon 5 / 4 | 5300 | `ForzaHorizon5` / `ForzaHorizon4` |
| Forza Motorsport 7 | 5300 | `ForzaMotorsport7` |
| Project Cars 2 / AMS2 | 5606 | `ProjectCars2` / `AMS2` |
| BeamNG.drive | 9999 / 63392 | `BeamNGDrive` |
| Euro Truck Simulator 2 | 25555 | — |
| X-Plane 11/12 | 49003 | — |
| Gran Turismo 7 | 33740 | — (console, use `--include-console`) |

---

## SimHub integration (target PC)

When telemetry from a new game first arrives on the target, sim-teleport:

1. Runs `SimHubWPF.exe -switchgame <code>` to tell SimHub which game is active.
2. Spawns stub processes (`acs.exe`, `acc.exe`, `AssettoCorsaEVO.exe`)
   so SimHub's plugin sees the expected game process running.
3. Resets when data goes stale (stale timeout fires) so the next session
   re-triggers the switch.

### SimHub auto-configuration

On the first run of `sim-teleport target`, it writes SimHub's
`GameSettings.json` so each game skips the "not configured" prompt, and
`HiddenGames.json` so the games appear in SimHub's game list. If changes
were written, the log prints:

```
[SimHub] Configured AssettoCorsa in GameSettings.json
[SimHub] *** Configuration updated. Please restart SimHub to apply changes. ***
```

**Restart SimHub after this message.** Subsequent runs are silent
(idempotent — it only writes when the file needs updating).

Files modified:
- `%APPDATA%\SimHub\PluginsData\GameSettings.json` — sets
  `ManualConfigurationDismissed`, `DisableConfigAlert`, and
  `AutomaticConfigurationDismissed` to `true` for each game
- `%APPDATA%\SimHub\PluginsData\HiddenGames.json` — unhides games

### Stub processes (why they exist)

SimHub's `ACSharedMemory.dll` calls `IsProcessRunning` before activating
its shared-memory reader. On the target PC no game process runs, so without
stubs SimHub silently skips reading shared memory even though the maps are
populated.

sim-teleport copies itself to the game's expected location and renames it
(e.g., `acs.exe`). When spawned with the `stub` argument, the copy just
sleeps forever. From SimHub's perspective, the process is running. When data
goes stale or sim-teleport shuts down, the stubs are killed.

A Windows Job Object (`JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE`) ensures stubs
are killed even if sim-teleport crashes — when the job handle is closed by the
OS on process termination, Windows kills all processes in the job.

**Stub placement**: stubs must live in the same directory that the Steam
appmanifest ACF's `installdir` field points to. SimHub's `FindProcessPath`
gets the running exe's path via `GetModuleFileNameExW`, then calls
`GetDirectoryName` on it — that path becomes the game's install root. If the
root doesn't match what the ACF says, SimHub's ACManager gets confused.

`steam::resolve_game_dirs` reads the actual `installdir` from whichever ACF
is on disk (real or stub-written) and passes those exact paths to
`StubManager`. The stub is placed there, matching the ACF exactly.

### Steam and fake ACF manifests

SimHub's ACManager reads game install paths from Steam's `appmanifest_*.acf`
files, not from the Windows registry. On the target PC where no games are
installed, sim-teleport writes fake ACF files so ACManager finds a valid path.

At startup, `sim-teleport target`:
1. Finds Steam via `HKEY_LOCAL_MACHINE\SOFTWARE\Valve\Steam\InstallPath`
   (or the WOW64 path for 32-bit Steam on 64-bit Windows).
2. Reads `steamapps\libraryfolders.vdf` to discover all Steam library roots.
3. For each library root, for each AC game: if no ACF exists (or if the
   existing install has ≤10 files, indicating a stub rather than a real
   install), writes `appmanifest_<appid>.acf` with `StateFlags=4`.
4. Creates the game directory structure under `steamapps\common\<installdir>\`.
5. Reads `installdir` back from the ACF to build the `game_dirs` map for stubs.
6. On shutdown, removes any ACF files it created.

If the game is genuinely installed (>10 files in the common directory),
the real ACF is left untouched and sim-teleport reads the real `installdir`.

### AC1 fake install structure

The following files are created in `steamapps\common\assettocorsa\`
(or whatever the real `installdir` is):

```
assettocorsa\
├── acs.exe                          ← stub executable (copy of sim-teleport.exe)
├── cfg\
│   └── python.ini                  ← [SIMHUB]\r\nACTIVE=1\r\n[SIMHUB_LOG]\r\nACTIVE=0
├── system\cfg\
│   └── assetto_corsa.ini           ← [SETTINGS]\r\n
├── apps\python\SimHub\
│   ├── SimHub.py                   ← # sim-teleport stub
│   ├── simhub_shared_mem.py        ← # sim-teleport stub
│   └── __init__.py                 ← # sim-teleport stub
└── content\
    ├── cars\                        ← empty (existence check)
    ├── tracks\                      ← empty
    ├── driver\                      ← empty
    ├── sfx\                         ← empty
    ├── fonts\                       ← empty
    └── gui\                         ← empty
```

**Why `cfg\python.ini`**: ACManager reads this file from the install root
to decide whether to activate the Python plugin system. Without it, the
SimHub Python app never initializes, which cascades into a NullReferenceException
before `GD_CarModel` is even reached.

**Why `system\cfg\assetto_corsa.ini`**: Required for ACManager initialization.

**Why `apps\python\SimHub\`**: The SimHub plugin files — ACManager validates
the plugin directory structure before enabling shared-memory reading.

**Why the empty `content\` subdirs**: ACManager existence-checks several
content subdirectories at init time. Missing directories cause early exits.

Documents folder: `setup_documents_folders` also creates:
- `%USERPROFILE%\Documents\Assetto Corsa\cfg\python.ini`
- `%USERPROFILE%\Documents\Assetto Corsa Competizione\Config\broadcasting.json`
- `%USERPROFILE%\Documents\Assetto Corsa EVO\`

These are the game's user-data locations, separate from the install root.

### Game announce protocol

AC Teleport source sends a `PAGE_GAME_ANNOUNCE` packet (buf_offset =
`0xFFFFFFFE`) immediately after detecting a game and every 30 seconds.
The 1-byte payload is the game ID: `0` = AC1, `1` = EVO, `2` = ACC.

The target receives this before the first telemetry frame. On receipt:
1. The correct stub process is spawned (`acs.exe`, `AssettoCorsaEVO.exe`,
   or `acc.exe`).
2. `SimHubWPF.exe -switchgame <code>` is called with the correct SimHub
   code (`AssettoCorsa`, `AssettoCorsaEVO`, or `AssettoCorsaCompetizione`).
3. Wrong stubs from previous detections are killed.

Why this is needed: the target PC doesn't run the game, so it can't probe
shared memory to determine which AC variant is active. The source does the
detection and broadcasts the result so the target can react correctly.

Old target binaries that don't recognize `PAGE_GAME_ANNOUNCE` silently skip
it (the `page_idx > 2` guard was already in place) — fully backward-compatible.

**Switchgame deduplication**: `ActiveGameTracker` remembers the last game
code it switched to. If the new code matches, `SimHubWPF.exe -switchgame`
is not called again. This is why EVO sending `AssettoCorsaEVO` (not
`AssettoCorsa`) matters — if both resolve to the same code, the EVO switch
would be silently dropped as a duplicate of the AC1 switch.

---

## FanaLab LED cleanup

FanaLab reads RPM data from shared memory and sends LED commands to Fanatec
wheel firmware. When a game exits, if shared memory still contains stale RPM
data, the LEDs stay lit indefinitely — even after disconnecting the game.

**The fix**: on game exit (clean shutdown signal), each telemetry engine
zeroes its shared memory region before closing:
- iRacing Teleport: writes zeros to the entire 1.1 MB map via
  `WriteProcessMemory` + `VirtualQuery`.
- AC Teleport: zeroes all three page maps (physics, graphics, static).
- Target side: both receivers zero their maps on stale timeout.

FanaLab reads RPM=0 on the next poll and sends the LED-off command to the
firmware.

**Important nuance**: this only fires on a **clean shutdown signal** (Ctrl-C,
Task Manager end task, or the `TargetSlot::stop()` shutdown channel). If
sim-teleport is killed via `TerminateProcess` (e.g., machine power loss), the
memory is not zeroed. The stale timeout (default 10 s) handles this case on
the target — after 10 s of no data the maps are zeroed.

**Why it doesn't fire on session transitions**: AC session transitions cause
`packetId` to briefly stop advancing (the game is between sessions). Without
careful gating, the cleanup would fire mid-session and clear maps while
the game is still running. The implementation only fires on the explicit
shutdown signal from the source (game process exited), not on stale detection.

---

## Per-game setup notes

### Wreckfest 2

Wreckfest 2 does not send telemetry by default. sim-teleport source creates
`config.json` automatically if you have run Wreckfest 2 at least once (so
the save directory exists). Restart the game after sim-teleport creates it.

If you prefer to create it manually:

Path: `%USERPROFILE%\Documents\My Games\Wreckfest 2\<SteamID>\savegame\telemetry\config.json`

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

`<SteamID>` is the numbered folder inside `My Games\Wreckfest 2\`. Restart
the game after creating the file.

**Port offset note**: sim-teleport forwards Wreckfest 2 on port
`23123 + 10000 = 33123` (default offset). The firewall rule on the SimHub PC
must include 33123.

### F1 25 / F1 24 / DiRT Rally 2.0 / WRC

Enable UDP telemetry in Game Options → Settings → Telemetry Settings. Set
port to `20777` and IP to `127.0.0.1`.

### Euro / American Truck Simulator

Install the [SCS SDK Telemetry Plugin](https://github.com/RenCloud/scs-sdk-plugin).

### Gran Turismo 7 (console)

GT7 runs on a PS4/PS5 — no PC process to detect. Pass `--include-console`
to the source to always forward port 33740.

---

## SimHub in-game app note

SimHub's Assetto Corsa in-game app creates `acpmf_simhub_v2` for opponent
tracking. This map is not forwarded by sim-teleport — only the three core maps
(`acpmf_physics`, `acpmf_graphics`, `acpmf_static`) are forwarded.

What works: speed, RPM, gear, throttle/brake, temperatures, tyre data, lap
times, position, session info. What does not work: opponent tracking and
leaderboard overlays that read `acpmf_simhub_v2`.

---

## Session reports

**Source** writes `sim-teleport-report.txt` next to the exe every 60 seconds
and on clean shutdown. Contents:
- Header: version, mode, config summary, start time, total runtime
- Detection summary: scan counts, probe hits, tiebreaker stats per game
- Session history: which game ran, how it was detected, start/stop times
- Errors and warnings logged during the session

**Target** writes `sim-teleport-target-report.txt` at startup (once). Contents:
- Steam library paths found
- Per-game ACF status (written, skipped because real install present, or
  Steam not found)
- Stub placement paths (`[steam] stub dir: acs → C:\...\common\assettocorsa`)

Both files are plain text, readable in any editor.

---

## CLI reference

### `sim-teleport source [OPTIONS]`

| Flag | Default | Description |
|------|---------|-------------|
| `--target <IP>` | — | SimHub PC's IP. Required for sim-relay; also for unicast iRacing/AC. |
| `--bind <IP>` | — | This PC's IP. Required for unicast (so resync packets from target reach the source). |
| `--unicast` | off | Direct ethernet mode — no multicast. |
| `--high-priority` | off | Set `HIGH_PRIORITY_CLASS` on telemetry threads. |
| `--busy-wait` | off | Spin-wait instead of sleeping. Lower jitter, burns one CPU core. |
| `--iracing-port <PORT>` | `5000` | iRacing Teleport UDP port. |
| `--ac-port <PORT>` | `5001` | AC Teleport UDP port. |
| `--no-iracing` | off | Disable iRacing Teleport. |
| `--no-ac` | off | Disable AC Teleport. |
| `--no-relay` | off | Disable Sim Relay. |
| `--scan-interval <SECS>` | `3` | Detection cycle interval. |
| `--drain <SECS>` | `20` | Grace period after game closes before stopping telemetry. |
| `--verbose` | off | Print detailed detection results each cycle. |
| `--port-offset <N>` | `10000` | Port offset for sim-relay (target listens on `game_port + N`). |

### `sim-teleport target [OPTIONS]`

| Flag | Default | Description |
|------|---------|-------------|
| `--source <IP>` | — | Gaming PC's IP. Passed to sim-relay for packet filtering. |
| `--unicast` | off | Direct ethernet mode. |
| `--high-priority` | off | Set `HIGH_PRIORITY_CLASS` on receiver threads. |
| `--busy-wait` | off | Spin-wait instead of sleeping. |
| `--iracing-port <PORT>` | `5000` | iRacing Teleport port. |
| `--ac-port <PORT>` | `5001` | AC Teleport port. |
| `--no-iracing` | off | Disable iRacing Teleport receiver. |
| `--no-ac` | off | Disable AC Teleport receiver. |
| `--no-relay` | off | Disable Sim Relay receiver. |
| `--fanalab` | off | Write iRacing data to FanaLab shared memory. |
| `--port-offset <N>` | `10000` | Port offset for sim-relay. |

### Other commands

| Command | Description |
|---------|-------------|
| `sim-teleport setup` | Interactive config wizard — creates `sim-teleport.toml`. |
| `sim-teleport install [--mode source\|target]` | Register auto-start at Windows logon (Task Scheduler, runs at highest privilege). |
| `sim-teleport uninstall` | Remove auto-start. |
| `sim-teleport list [--verbose]` | Show all supported games. |
| `sim-teleport firewall` | Print copy-paste PowerShell firewall rules. |
| `sim-teleport --version` | Show version including sub-app versions. |
| `sim-teleport stub` | *Internal.* Sleep forever. Used as the renamed stub process. |

---

## Configuration file

CLI flags always override `sim-teleport.toml`. Run `sim-teleport setup` for a
guided wizard, or create the file manually:

```toml
# sim-teleport.toml — example for direct ethernet

[network]
unicast    = true
source_ip  = "192.168.50.1"   # gaming PC
target_ip  = "192.168.50.2"   # SimHub PC

[apps]
high_priority = true
busy_wait     = true

[simhub]
# path = "C:/Program Files (x86)/SimHub/SimHubWPF.exe"  # uncomment if needed
```

### All config fields

| Field | Default | Description |
|-------|---------|-------------|
| `mode` | `"source"` | `"source"` or `"target"`. Used by `sim-teleport install`. |
| `network.unicast` | `false` | `true` = unicast (direct cable). `false` = multicast (LAN). |
| `network.source_ip` | `"192.168.50.1"` | Gaming PC IP. |
| `network.target_ip` | `"192.168.50.2"` | SimHub PC IP. |
| `ports.iracing_teleport` | `5000` | iRacing Teleport UDP port. |
| `ports.ac_teleport` | `5001` | AC Teleport UDP port. |
| `detection.scan_interval` | `3` | Detection cycle in seconds (source). |
| `detection.drain_seconds` | `20` | Grace period after game closes. |
| `apps.iracing_teleport_enabled` | `true` | Enable iRacing Teleport. |
| `apps.ac_teleport_enabled` | `true` | Enable AC Teleport. |
| `apps.sim_relay_enabled` | `true` | Enable Sim Relay. |
| `apps.high_priority` | `false` | `HIGH_PRIORITY_CLASS` on telemetry threads. |
| `apps.busy_wait` | `false` | Spin instead of sleeping. |
| `apps.fanalab` | `false` | Write iRacing data to FanaLab memory (target). |
| `apps.relay_port_offset` | `10000` | Sim Relay port offset. BeamNG OutGauge (port 63392) overflows — set ≤ 2143 for that game. |
| `advanced.stale_timeout_secs` | `10` | Seconds without data before marking telemetry stale. |
| `advanced.reconnect_timeout_secs` | `10` | iRacing source reconnect timeout. |
| `advanced.ac_poll_rate` | `60` | AC Teleport source poll rate (Hz). |
| `advanced.datagram_size` | `9000` | iRacing Teleport datagram size. Use `1472` on standard MTU links. |
| `simhub.path` | *(default install)* | Path to `SimHubWPF.exe`. |
| `simhub.iracing` | `"iRacing"` | SimHub game code for iRacing. |
| `simhub.ac` | `"AssettoCorsa"` | SimHub game code for AC1. Also the fallback for EVO/ACC if their specific codes are unset. |
| `simhub.ac_evo` | — | SimHub game code for AC EVO sessions. Default: `"AssettoCorsaEVO"`. |
| `simhub.acc` | — | SimHub game code for ACC sessions. Default: `"AssettoCorsaCompetizione"`. |
| `simhub.relay.<id>` | — | SimHub code for a sim-relay game. Key is the sim-relay game ID. Only needed to override built-in codes or add unsupported games. |

Built-in SimHub codes (no config needed):

| sim-relay ID | SimHub code |
|-------------|-------------|
| `wreckfest2` | `Wreckfest2` |
| `f1-25` … `f1-20` | `F12025` … `F12020` |
| `dirt-rally2` | `DirtRally2` |
| `dirt5` | `Dirt5` |
| `wrc-24` / `wrc-23` | `WRC2024` / `WRC2023` |
| `beamng-sh` / `beamng-outgauge` | `BeamNGDrive` |
| `pcars2` / `kartkraft` | `ProjectCars2` |
| `ams2` | `AMS2` |
| `forza-fm` | `ForzaMotorsport` |
| `forza-fh4` / `forza-fh5` | `ForzaHorizon4` / `ForzaHorizon5` |
| `forza-fm7` | `ForzaMotorsport7` |

---

## Auto-start (Task Scheduler)

```
sim-teleport install                    # mode from sim-teleport.toml, or "source"
sim-teleport install --mode target      # force target mode
sim-teleport uninstall
```

Run as Administrator. The registered command is `sim-teleport.exe source` (or
`target`) with no flags — persistent settings come from `sim-teleport.toml`.

---

## Running SimHub locally

If SimHub is also on the gaming PC, `sim-teleport source` sends the same
data to the remote SimHub PC independently — no conflict. Do not run
`sim-teleport target` on the gaming PC — it creates shared-memory maps with
the same names as the game, which conflicts.

---

## Building from source

Requires Rust (stable). CI builds on `windows-latest`.

```
git clone --recurse-submodules https://github.com/t-hovestadt/sim-teleport.git
cd sim-teleport
cargo build --release
```

Cross-compile for Windows from macOS/Linux:

```
rustup target add x86_64-pc-windows-gnu
brew install mingw-w64   # macOS
cargo build --release --target x86_64-pc-windows-gnu
```

`#[cfg(windows)]` code is invisible to Linux/macOS clippy. Always run:

```
cargo clippy --target x86_64-pc-windows-gnu -- -D warnings
```

before pushing any Windows-specific code.

---

## Troubleshooting

| Symptom | Cause / Fix |
|---------|-------------|
| SimHub shows no AC telemetry after first run | Restart SimHub — `sim-teleport target` updated GameSettings.json. |
| `[SimHub] Configuration updated` on every run | SimHub restoring defaults; try a newer SimHub version. |
| `[simhub] switched to …` missing | `SimHubWPF.exe` not found at default path; set `simhub.path` in config. |
| AC EVO shows "waiting for data" in SimHub | Stub placed in wrong directory; check `sim-teleport-target-report.txt` for `[steam] stub dir:` line. |
| SimHub shows wrong game overlay | Game not in built-in code table; add to `[simhub.relay]`. |
| AC1 NullReferenceException in SimHub | Fake install incomplete; check target log for `[steam]` errors. |
| 0.2 msg/s on AC EVO (vs 60+ for AC1) | EVO game is on the menu, not in a session — `packetId` only advances in-session. Normal behavior. |
| 0 msg/s on target | Data not reaching target — check source log, firewall, and network path. |
| FanaLab LEDs stuck after game exits | Stale RPM data. Ensure `stale_timeout_secs` fires and zeroing runs. A 10-second delay is normal. |
| Wreckfest 2 not detected | `config.json` missing or game not restarted after creation. Check source log for `Created telemetry config`. |
| `[Wreckfest 2] Created telemetry config` | Restart Wreckfest 2 to activate telemetry — it only reads config on launch. |
| BeamNG OutGauge (port 63392) not working | Port overflows at `relay_port_offset = 10000`. Set `apps.relay_port_offset` to ≤ 2143. |
| Task Scheduler: sim-teleport starts but does nothing | Config flags are only read from `sim-teleport.toml` in auto-start mode; check that file is present next to the exe. |

---

## Tag and release convention

| Repo | Tag | Notes |
|------|-----|-------|
| sim-teleport | `v0.2.0` | Stays at HEAD; moved on each release |
| iracing-teleport | `v1.0` | Moves to HEAD on every update; never create `v1.0.x` tags |

Release workflow triggers on `push: tags: v*`. CI runs on `windows-latest`.
The release artifact (`sim-teleport.exe`) is built with
`--target x86_64-pc-windows-msvc`.

---

## Companion projects

| Repo | Purpose |
|------|---------|
| [iracing-teleport](https://github.com/t-hovestadt/iracing-teleport) | iRacing shared-memory streaming (standalone) |
| [ac-teleport](https://github.com/t-hovestadt/ac-teleport) | Assetto Corsa shared-memory streaming — archived; absorbed into sim-teleport as `crates/ac-teleport` |
| [sim-relay](https://github.com/t-hovestadt/sim-relay) | UDP relay for 35+ games — archived; absorbed into sim-teleport as `crates/sim-relay` |

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

---

## Auto-start (Task Scheduler)

```
sim-bridge.exe install    # registers "run at logon, highest privilege"
sim-bridge.exe uninstall  # removes the entry
```

Run as Administrator for install/uninstall.

---

## Configuration — `sim-bridge.toml`

Created automatically on first run, next to `sim-bridge.exe`.

| Key | Default | Description |
|-----|---------|-------------|
| `network.source_ip` | `192.168.50.1` | Gaming PC IP |
| `network.target_ip` | `192.168.50.2` | SimHub PC IP |
| `ports.iracing_teleport` | `5000` | iRacing Teleport port |
| `ports.ac_teleport` | `5001` | AC Teleport port |
| `detection.scan_interval` | `3` | Process scan interval (seconds) |
| `detection.drain_seconds` | `20` | Grace period after game closes |
| `apps.iracing_teleport_enabled` | `true` | Enable/disable iRacing support |
| `apps.ac_teleport_enabled` | `true` | Enable/disable AC/ACE/ACC support |
| `apps.sim_relay_enabled` | `true` | Enable/disable Sim Relay |

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

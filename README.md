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

---

## Auto-start (Task Scheduler)

```
sim-bridge.exe install           # registers at logon for mode stored in config
sim-bridge.exe install --mode target  # force target mode
sim-bridge.exe uninstall         # removes the entry
```

Run as Administrator for install/uninstall.

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

---

## Direct ethernet setup

For lowest latency, connect the two PCs with a dedicated ethernet cable (no switch).

**1. Assign static IPs**

On each PC: Network Adapter → Properties → IPv4 → Use the following IP address:

| PC | IP Address | Subnet Mask | Default Gateway |
|----|-----------|-------------|-----------------|
| Gaming (source) | `192.168.50.1` | `255.255.255.0` | *(leave blank)* |
| SimHub (target) | `192.168.50.2` | `255.255.255.0` | *(leave blank)* |

**2. Windows Firewall — allow inbound UDP on the gaming PC**

Open PowerShell as Administrator and run:

```powershell
# iRacing Teleport
New-NetFirewallRule -DisplayName "sim-bridge iRacing (UDP 5000)" `
    -Direction Inbound -Protocol UDP -LocalPort 5000 -Action Allow

# AC Teleport
New-NetFirewallRule -DisplayName "sim-bridge AC Teleport (UDP 5001)" `
    -Direction Inbound -Protocol UDP -LocalPort 5001 -Action Allow
```

Sim Relay games use their native ports (20777 for F1, etc.) — add rules as needed
for the specific games you play. Run `sim-bridge list` to see all ports.

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

# Changelog

## v0.2.6

### Fixed

- iRacing process-gone threshold increased from 9 seconds to 30 seconds — session transitions (practice → qualify → race) no longer trigger premature shutdown and restart loops

---

## v0.2.5

### Fixed

- iRacing session transitions no longer trigger the failure tracker — process flicker during loading screens (practice → qualify → race) was incorrectly counted as crashes, disabling iRacing forwarding for 5 minutes

---

## v0.2.4

### Fixed

- **FanaLab LED cleanup**: `zero_on_exit` and `zero_named_map` now retry up to 3 times
  with 5-second delays when the game's shared memory map can't be opened for writing.
- **FanaLab LED cleanup**: post-zero sleep increased from 200ms to 500ms to give FanaLab
  more time to read the zeroed RPM value before map handles are released.
- **FanaLab LED cleanup**: both functions now log success or failure with specific error
  details instead of failing silently after the caller already printed "Zeroing shared
  memory".
- **Reverted automatic Fanatec service restart** — too invasive and unreliable.
- **Detection**: SimHub on source PC no longer causes false AC1 detection when AC EVO
  maps exist but the game process is not confirmed.

### Changed

- Removed `--no-fanatec-restart` flag (no longer needed after revert above).

---

## v0.2.3

### Changed

- Reverted automatic Fanatec service restart — too invasive and unreliable in practice.
- Removed `--no-fanatec-restart` flag (no longer needed).

### Fixed

- **Documentation**: simplified FanaLab + AC EVO troubleshooting to a manual workaround:
  close FanaLab before launching AC EVO, reopen after EVO is running.

---

## v0.2.2

### Fixed

- **AC EVO: FanatecService shared memory conflict** — `sim-teleport source` now stops and
  restarts Fanatec Windows services when running as administrator, allowing AC EVO to create
  its shared memory maps without "Access is denied" errors. When not elevated, a warning is
  logged with manual steps instead.

- **AC EVO: target opens maps held by other processes** — target now calls `OpenFileMappingW`
  before `CreateFileMappingW`, so if FanaLab already holds a map open (write-accessible),
  sim-teleport writes into it rather than failing.

- **AC EVO: target creates maps at game-native sizes** — EVO maps are now created at
  4096 / 8192 / 4096 bytes (matching what AC EVO writes) instead of the AC1 uniform 64KB
  size. Eliminates oversized allocations and potential page-boundary issues.

- **Detection: no false AC1 trigger from SimHub-created maps** — when AC EVO shared memory
  maps exist on the source PC (created by SimHub or FanaLab) but the game process is not
  confirmed, the detector now suppresses the AC1/ACC check, preventing a false positive.

- **Troubleshooting: CarNames.csv missing key documented** — added entry explaining that
  the `Missing key 0 in LookupTables\AssettoCorsaEVO.CarNames.csv` SimHub log message is
  cosmetic and does not affect telemetry data.

### Added

- **`--no-fanatec-restart` flag** — source mode: disables automatic Fanatec service
  stop/start around game launch for users who prefer to manage FanaLab themselves.

- **Diagnostic hex dump for AC EVO maps** — source and target log the first 32 bytes of
  `acevo_pmf_physics` every 5 seconds while the game is active, to assist with shared
  memory debugging (temporary, will be removed after testing).

---

## v0.2.1

### Fixed

- **SimHub iRacing game code**: `simhub_setup.rs` was writing `"IRacing"` (capital R) into
  `GameSettings.json` and creating `PluginsData/IRacing/`. SimHub's internal game code is
  `"iRacing"` (lowercase r), which is what the `-switchgame` command sends. The mismatch
  meant the pre-configured settings and the active game slot never matched. Fixed: correct
  capitalisation throughout.

- **`sim-teleport list --verbose` iRacing detection string**: displayed
  `"Named event: IRSDKDataValidEvent"` — the reverted event-probe approach. Actual detection
  is a ToolHelp32 process scan for `iRacingSim64DX11.exe`. Fixed to show
  `"Process scan: iRacingSim64DX11.exe"`.

- **Session report "iRacing event probes" label**: the source-mode session report printed
  "iRacing event probes" for the detection counter; the field counts process scan cycles,
  not event probes. Fixed label to "iRacing process scans".

- **Binary version mismatch**: `Cargo.toml` was left at `0.1.5` after tagging v0.2.0, so
  `CARGO_PKG_VERSION` embedded in the target setup report said "0.1.5". Bumped to `0.2.0`
  in the previous commit; now at `0.2.1`.

### Updated

- **Template config files** (`sim-teleport.lan.toml`, `sim-teleport.direct.toml`): added
  `relay_port_offset = 10000` to `[apps]` and a full `[simhub]` section with commented-out
  override fields, matching what the setup wizard generates.

- **Release assets**: removed `README.md` from the GitHub Release download list (it is
  already visible on the repository page and is not a useful download artifact).

---

## v0.2.0

### Breaking changes

- **Renamed to sim-teleport**: binary is now `sim-teleport.exe`, config file is
  `sim-teleport.toml`, log is `sim-teleport.log`, reports are
  `sim-teleport-report.txt` / `sim-teleport-target-report.txt`, stub temp
  directory is `%TEMP%\sim-teleport-stubs\`, Task Scheduler entry is
  `SimTeleport`, and firewall rule display names are `"sim-teleport source"` /
  `"sim-teleport target"`. The binary loads `sim-bridge.toml` as a fallback with
  a migration warning if `sim-teleport.toml` is not found. `uninstall` also
  removes the legacy `SimBridge` Task Scheduler entry if present.

### Internal

- **Monorepo consolidation**: ac-teleport and sim-relay absorbed as Cargo
  workspace crates (`crates/ac-teleport`, `crates/sim-relay`). Both external
  repos are archived. Only iracing-teleport remains as a git submodule under
  `deps/`. No functional changes; `git clone --recurse-submodules` still works.

- **`src/source.rs` split**: 976-line monolith split into four focused modules
  (`mod.rs`, `detection.rs`, `slot.rs`, `wreckfest.rs`) with no logic changes.

---

## v0.1.5

### SimHub integration — AC games

- **Steam ACF approach**: replaced HKLM registry entries (which required UAC
  elevation on first run) with fake Steam `appmanifest_*.acf` files. SimHub's
  ACManager reads install paths from `steamapps\appmanifest_<appid>.acf`, not
  from the registry. sim-bridge now writes fake ACFs so ACManager finds a valid
  path without any admin prompt. ACF files are removed on clean shutdown.

- **`src/steam.rs`** — new module: Steam library discovery via registry +
  `libraryfolders.vdf` parsing; fake ACF writing; `resolve_game_dirs` reads the
  real `installdir` from whatever ACF is on disk (real or fake) so stubs are
  placed in the exact directory ACManager will look for.

- **Stub placement fix**: `StubManager` now uses per-game dirs from
  `resolve_game_dirs` instead of `steamapps\common\ + hardcoded subdir`. Fixes
  "ACEVOManager waiting for data" when EVO's real Steam `installdir` differs from
  the hardcoded fallback (e.g., "Assetto Corsa EVO" vs "assettocorsa_evo").

- **AC1 fake install skeleton** expanded: `cfg\python.ini` added (ACManager reads
  this to enable the Python plugin system); empty `content\tracks\`,
  `content\driver\`, `content\sfx\`, `content\fonts\`, `content\gui\`
  directories added alongside existing `content\cars\`.

- **EVO switchgame fix**: source was sending `"AssettoCorsa"` for EVO sessions
  instead of `"AssettoCorsaEVO"`. `ActiveGameTracker` was deduplicating the EVO
  switch as a no-op because it matched the already-active AC1 code. Fixed:
  EVO now sends `"AssettoCorsaEVO"` (or the `simhub.ac_evo` config override).

- **ACC switchgame**: ACC sessions were sending no `switchgame` command at all.
  Fixed: ACC now sends `"AssettoCorsaCompetizione"` (or `simhub.acc` override).

- **`simhub.acc` config field**: new optional field to override the SimHub game
  code for ACC sessions (default: `"AssettoCorsaCompetizione"`).

- **Game announce protocol**: `PAGE_GAME_ANNOUNCE` packet (from ac-teleport)
  carries `GAME_ID_AC1`, `GAME_ID_EVO`, or `GAME_ID_ACC`. Target uses this to
  spawn the correct stub process and send the correct switchgame code. Prevents
  the target from defaulting to AC1 behavior when EVO or ACC data arrives.

- **Target setup report**: `sim-bridge-target-report.txt` written at startup
  showing Steam library paths, per-game ACF status, and stub placement dirs.

### Bug fixes

- **Process flickering**: AC session transitions briefly remove game processes
  from the process list. Added 3-consecutive-scan liveness requirement before
  sending shutdown signal, preventing spurious reconnects during AC loading.

- **FanaLab detection revert**: iRacing detection reverted from
  `IRSDKDataValidEvent` probe back to `iRacingSim64DX11.exe` process scan.
  FanaLab holds the event handle open after iRacing exits, causing ghost
  detection. Process-name detection is immune to this.

### Diagnostics

- **`--verbose` EVO hex dump**: ac-teleport source prints first 64 bytes of
  `acevo_pmf_physics` and `acevo_pmf_graphics` maps once per second when
  `--verbose` is set. Used to locate `packetId` offset in EVO v0.6 struct
  layout changes.

### AC EVO v0.6 compatibility

- Confirmed: ac-teleport is a raw byte tunnel. Source reads maps via
  `VirtualQuery` (actual OS-reported size), LZ4-compresses the entire slice,
  target decompresses and writes raw bytes. No struct parsing anywhere in
  the pipeline. EVO v0.6 struct changes have zero effect — SimHub on the
  target reads the maps with its own v0.6-aware struct definitions.

---

## v0.1.4

Initial public release. Bundled iracing-teleport, ac-teleport, and sim-relay
with unified game detection, SimHub auto-switching, and sim-relay auto-detect mode.

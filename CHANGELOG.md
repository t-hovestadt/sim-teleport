# Changelog

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

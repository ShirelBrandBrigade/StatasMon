# Known issues

Open items that are real but not blocking the current phase. Remove an
entry when it is fixed; reference the commit.

- **Dev-only console warnings on first mount** (2026-09-09, Phase 1): React
  reports a missing list key attributed to `ThermalCard` and react-virtuoso
  sets `paddingBottom: NaN` once on the log list's first paint. Neither
  recurs on re-render or device switching, and neither appears in Vitest.
  Investigate during Phase 6 (Logs screen) when the log list is reworked.
- **Storage "GB" is binary** (2026-09-09): `formatValue` scales bytes by
  1024, so a 256 GB (decimal) device shows 238.4 GB. Decide in Phase 3
  whether storage uses decimal units like the vendors (protocol change:
  a `bytes_decimal` unit).
- **Fake Android emits iOS-style memory fields** (`mem.app`, `mem.wired`),
  so the Memory card shows the iOS breakdown for the fake tablet. The real
  Android provider (Phase 2) emits `mem.cached`/`mem.swap` instead.
- **`useTelemetryTick` throttles with requestAnimationFrame**, which never
  fires in a hidden window; live cards catch up in one tick when the window
  is visible again. Acceptable for v1; revisit if background recording UI
  needs live counters.
- **Android tick cost is ~270 ms through the adb subprocess** (2026-09-09):
  each cadence spawns one `adb shell` per tick; ~50 ms of that is process
  spawn. A persistent shell or a native host-protocol client (ADR-009's
  "later optimization") would bring the fast tick under ~100 ms. Fine at
  1 Hz; revisit before Phase 7's perf pass.
- **Samsung `dumpsys battery` prints `current now: 671093`** with an
  unverified unit and sign, so `bat.current` and `bat.power` are withheld
  on this device until checked against a known charging load.
- **Deferred Android sub-items**: opt-in `dumpsys meminfo` PSS snapshots
  (plan 2.5) and enrichment of crash/ANR events from `dumpsys dropbox`
  (plan 2.6) are not implemented; events currently carry the log line only.
- **Wireless ADB pair/connect and bugreport** run real adb but were not
  exercised live (they change host/device state); the dialog and the
  actions are unit-tested.
- **iOS live verification pending** (2026-09-09): the iPad was not connected
  while Phase 3 was built. Everything is tested against fixtures and the
  documented plist shapes. With the iPad plugged in and trusted, run:
  `STATASMON_LIVE_IOS=1 cargo test -p statasmon-device-ios --tests -- --nocapture`
  and `cargo run -p statasmon-device-ios --example capture_fixture` (replaces
  the shape-only fixtures under fixtures/ios), then
  `cargo run -p statasmon-device-ios --example capture_logs_fixture`. Open
  questions the hardware answers: whether usbmuxd accepts SavePairRecord
  from us, the exact IOPMPowerSource keys/units on iPadOS 17.4.1, whether
  MobileGestalt reports PhysicalMemory, and the thermalmonitord wording.
- **Store follow-ups** (2026-09-09): add an `EngineError::Store` variant
  (store errors currently map to Device/Io); per-app storage sizes on iOS
  have no protocol message yet (`IosSource::app_sizes()`); wide CSV export
  switches to long format above 64 metrics (ADR-012).
- **DMG bundling on this Mac** (2026-09-09): Tauri's `bundle_dmg.sh` fails
  with "Permission denied" when run from the agent shell; it succeeds with
  `CI=true` (skips the Finder AppleScript step), and `hdiutil create`
  produces an equivalent DMG. CI is unaffected.
- **Low host memory** during the final test runs: Vitest workers were killed
  when the machine had under 100 MB free; run `vitest run --maxWorkers=2`
  when that happens. Not an app issue.

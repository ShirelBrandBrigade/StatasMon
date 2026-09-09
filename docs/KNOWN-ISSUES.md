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
- **Real-device verification pending** for everything in Phase 1 (by
  design: fake device first).

# Decision log (ADRs)

One entry per decision. Newest last. Format: date · title · status ·
context · decision · consequences. Revisit by adding a new entry, never by
editing an old one.

## ADR-001 · 2026-09-09 · Visual direction: AD-1 "Dark Glass"
Status: accepted. Context: six directions and three A+D blends were mocked
up; the user picked AD-1. Decision: `docs/DESIGN-SYSTEM.md` is derived from
it and governs all UI. Consequences: dark-only theme for v1; light theme is
a later token set, not a redesign.

## ADR-002 · 2026-09-09 · Multi-device and Android are v1 scope
Status: accepted. Context: user requires several devices connected at once
and Android support alongside iPhone/iPad. Decision: the core is a
provider-per-device model from day one; the UI never assumes one device.
Consequences: per-device sessions and stores; sidebar device list is a
primary control.

## ADR-003 · 2026-09-09 · Shell and core: Tauri 2 + Rust
Status: accepted. Context: needs USB/QUIC/SQLite access, small installers,
macOS + Windows, sidecar support. Decision: Tauri 2 with a Rust core;
React 19 + TypeScript UI; uPlot and react-virtuoso for the hot paths.
Consequences: no Node in the backend; Rust cold builds are slower; test on
both WKWebView and WebView2.

## ADR-004 · 2026-09-09 · iOS library: `idevice` (Rust, MIT) primary
Status: accepted. Context: pymobiledevice3 is GPL-3 and Python;
libimobiledevice cannot reach iOS 17+ instruments; go-ios is Go and needs a
kernel tunnel or agent. Decision: `idevice` crate in-process, userspace
tunnel; go-ios kept as a fallback sidecar option (Phase 4.7).
Consequences: pre-0.2 crate, expect breaking changes; fixtures per iOS
major; tier-1 (no tunnel) features must always work.

## ADR-005 · 2026-09-09 · Android: adb binary + Rust server-protocol client
Status: accepted. Context: prebuilt platform-tools cannot be redistributed;
pure USB clients re-implement auth and quirks. Decision: `adb_client` in
server-proxy mode against an adb found on the machine or downloaded on
first run with the user's consent; reuse a running server. Consequences:
first-run download step; AOSP-built adb is a later option (needs an ADR).

## ADR-006 · 2026-09-09 · iOS thermal state is derived from device logs
Status: accepted. Context: no host-side thermal query API on iOS; only
battery temperature is readable. Decision: parse thermalmonitord
transitions from os_trace; show "not reported yet" until the first
transition and label the source. Consequences: state may be stale after
connect; SoC temperature is dashed on iOS.

## ADR-007 · 2026-09-09 · Packet capture on Android is out of v1
Status: accepted. Context: tcpdump needs root on user builds. Decision:
pcap only on iOS (pcapd); Android shows a dash with the reason.

## ADR-008 · 2026-09-09 · Sessions auto-record on attach; pausing stops storage, not the connection
Status: accepted. Context: the engine (step 0.4) needed a policy. Decision:
`EngineConfig::auto_record` defaults to true so a device starts recording
the moment it attaches; while Paused or Stopped the live snapshot still
updates (latest values, cores, processes) but nothing is stored, counted or
forwarded as `Samples`/`Cores`/`Processes`/`Log`; `Info`, `Capabilities`,
`DeviceState`, `Event`, `Error`, `Headline` and `Session` always flow.
Consequences: the UI can show live numbers with a "paused" badge; storage
(Phase 5) follows the same gate.

## ADR-009 · 2026-09-09 · Android transport is the adb binary as a subprocess
Status: accepted; supersedes the client-crate part of ADR-005 (the
"adb found on the machine or downloaded with consent, reuse a running
server" part stands). Context: step 2.1–2.2 needed a transport now. A
Rust host-protocol client (`adb_client`) adds a dependency that must track
adb's USB, TLS-PSK and per-OEM quirks, while the adb binary is already
required for the server. Measured on the Galaxy Tab S8: ~50 ms per
`adb -s <serial> shell …` call on macOS, and every tick batches its
commands into one `sh -c`, so the overhead is one process spawn per
cadence per tick. Decision: `ProcessShell` (spawn `adb shell`), device
tracking by polling `adb devices -l` every 2 s, `adb pair`/`adb connect`
for wireless; the server port is whatever adb uses (`ANDROID_ADB_SERVER_PORT`
is inherited, so a second server never fights Android Studio). The connect
probe is one batched command with sentinel sections and never enumerates
`/sys/class/thermal` (9 s on Samsung). Core-type heuristic: the cpufreq
policy group with the lowest `cpuinfo_max_freq` is "efficiency", every
other policy is "performance" (1+3+4 Snapdragon → 4P + 4E). Consequences:
zero new crates; a native host-protocol client (`host:track-devices`,
persistent shells) is a later optimisation behind the same `Shell` trait;
`nproc` is not used for core counts because it honours the shell's cpuset
(6 of 8 on the Tab S8) — `/sys/devices/system/cpu/possible` is.

## ADR-010 · 2026-09-09 · Android sampler conventions
Status: accepted. Context: step 2.3–2.6 implementation choices that the
UI and later providers must share. Decisions: per-process `cpu_pct` uses
the `top` convention (100 % = one core busy); `mem_bytes` = RSS from `ps`
(KiB × 1024); memory mapping used = MemTotal − MemAvailable, cached =
Cached + Buffers, app = AnonPages, wired = SUnreclaim + KernelStack +
PageTables, swap = SwapTotal − SwapFree; `mem.pressure` from PSI `some
avg10` with thresholds 1 / 5 / 20 % (fallback: major-fault rate, capped at
2); `net.dropped` is the kernel running total, `net.rx_total`/`tx_total`
count from the first tick; core kind = highest-max-frequency cluster is
Performance, the rest Efficiency (so the SM8450 reads 4P + 4E); metrics
the device cannot provide are not emitted (no zeros). Battery detail is
marked available when `dumpsys battery` works even if the sysfs is locked;
the missing fields (cycles, capacities) are dashed individually.
Consequences: the Android tablet shows real per-core load, three thermal
sensors and disk categories; `bat.current` is withheld until Samsung's
`current now` unit and sign are verified against a known load.

## ADR-011 · 2026-09-09 · iOS pairing UX and provider shape (tier 1)
Status: accepted. Context: step 3.1 needed a trust flow that never blocks
and a way for every iOS service (battery, storage, logs, crashes, pcap) to
share one pair record. Decisions: (1) `MuxdDiscovery` attaches a device
the moment usbmuxd reports it; without a valid pair record it is attached
as an `IosSource::untrusted` (`ConnectionState::NeedsTrust`, every
capability unavailable with the reason) and a lockdown `Pair` request is
sent so the device shows "Trust This Computer"; the request is repeated
every 3 s while `PairingDialogResponsePending`, the reason changes to
"unlock the device" on `PasswordProtected`, and after `UserDeniedPairing`
the device is asked again only every 30 s until it is re-plugged. Once
paired the record is saved into usbmuxd (`SavePairRecord`, so iTunes /
Finder / other tools share it) and the untrusted source is replaced by the
tier-1 source. The host id is a fresh uppercase UUID per pairing, as
Apple's tools do. (2) `IosProvider` wraps `UsbmuxdProvider` and caches the
pair record in memory, so a service open costs one usbmuxd round-trip;
the logs agent receives it as `Arc<dyn IdeviceProvider>`. (3) usbmuxd
`Listen` runs on a dedicated thread (the crate's stream is not `Send`);
when `Listen` is refused the discovery polls `ListDevices` every 2 s.
(4) A tier-1 source emits only what it measured: RAM, uptime, media/other
storage and all DVT metrics are dashed with the Developer Mode / tunnel
reason; `BatteryDetail` is available only when the IORegistry query
answered at connect. Dependencies added to `device-ios`: `uuid` (already
a workspace dependency, MIT/Apache-2.0) and `futures` 0.3 (MIT/Apache-2.0,
already pulled in by `idevice`). Consequences: the sidebar shows an iOS
device within a second of plugging it in, with a "needs trust" state
instead of nothing; a pairing declined on the device is not retried in a
loop; the per-app storage list has no protocol message yet (kept on
`IosSource::app_sizes`, to be added to the protocol in Phase 6).

## ADR-012 · 2026-09-09 · Session store, retention and export formats
Status: accepted. Context: Phase 5 (5.1, core of 5.2 and 5.4) needed the
on-disk shape of sessions, the write path that never touches the UI
thread, and the export formats the UI will offer. Decisions:
- **Layout.** Under the app data folder (`~/Library/Application
  Support/com.statasmon.app` on macOS via Tauri's resolver;
  `STATASMON_STORE_DIR` overrides it for dev runs): `index.sqlite` with one
  `sessions` row per session (the `SessionSummary` fields plus
  `device_info_json`, `last_ts_ms`, `path`) and `sessions/<id>.sqlite` with
  `samples(ts_ms, metric, value)` indexed `(metric, ts_ms)`, `cores`,
  `processes`, `logs(seq PRIMARY KEY, …)`, `events`, `marks`, `meta`. Both
  are WAL with `synchronous=NORMAL`; schemas are versioned by
  `PRAGMA user_version` (index v1, session v1) and migrated on open.
  Enum columns store the wire names (`error`, `lmk_kill`, `performance`).
- **Session ids** are `<device_id sanitised>-<yyyyMMdd-HHmmss>` (local
  time; `:`/`/` become `_`), made unique with `-2`, `-3` when needed, and
  are the ids the UI sees in `SessionState.id`; the in-memory `Session`
  keeps its uuid internally.
- **Writer.** One `std::thread` per store fed by a bounded
  `std::sync::mpsc` queue (4 096 ops). Everything that arrives within one
  second is applied in one transaction per session; index counters are
  folded in the same batch. The engine's `write` never blocks: a full queue
  drops the message and counts it (`Store::dropped`, a warning every 1 000
  drops); lifecycle ops (`begin`, `end`, `mark`, `set_info`, `flush`) wait
  for room. ADR-008 applies: nothing is stored while paused except the
  session transition events themselves and marks.
- **Retention defaults.** `max_session_ms` 4 h: when a session's recording
  time reaches it the engine stops it and starts a new one (`Event`
  "Session split…" + `Session` message); `keep_days` 30: a daily sweep on
  the writer thread deletes ended sessions older than that, then the
  oldest ended sessions past the optional `max_total_bytes`; every sweep
  ends with `wal_checkpoint(TRUNCATE)`. Sessions left open by a crash are
  ended at their last sample when the store opens. Deleting a live session
  is refused by the store; the engine stops it first.
- **Queries.** History is downsampled in SQL (`GROUP BY` bucket of
  `step_ms` from `from_ms`, average, stamped with the bucket start — the
  same shape the live ring returns) and never returns more than 5 000
  points: the step is raised when a range would. Logs filter by process,
  level, time range and buffer like the live ring. Substring search uses an
  **FTS5 external-content table with the trigram tokenizer** (available in
  the bundled SQLite; verified by the store tests) for queries of three or
  more characters and `LIKE … ESCAPE '\'` otherwise or when FTS is
  missing (`meta.fts` records which); regex search uses a `regexp`
  function registered on the connection (case-insensitive, 1 MiB size
  limit). Readers open their own connection per query.
- **Export.** Every export writes `<path>.part` and renames it into place
  when complete (an existing file is replaced), reports progress about
  every 2 % through `Message::Progress { task_id: "export-<n>" }` and
  returns `ExportResult { path, bytes, entries }`. `.log`: one line per
  entry — `2026-09-09 14:32:07.412  FieldKit[4412]  ERROR
  com.example.fieldkit  message` (local time, origin = iOS subsystem or
  Android tag). `.jsonl`: header `{"statasmon":1,"session":…}` then one
  `LogEntry` per line. `.csv`: **wide** (`ts_ms,cpu.total,…`, one row per
  tick) for the `Session` scope with ≤ 64 metrics — the whole catalog fits,
  so 40 (the first draft) would have made wide unreachable on real devices
  — **long** (`ts_ms,metric,value`) otherwise; the `App` scope exports the
  app's process rows (the only per-app metrics stored). `.zip` bundle:
  `device.json`, `session.json`, `metrics.csv` (wide), `cores.csv`,
  `processes.csv`, `logs.jsonl`, `events.json`, `marks.json`, `crashes/*`
  copied from `<app-data>/crashes/<device>/` (sanitised or raw device id),
  `capture.pcap` from `<app-data>/pcap/<session id>.pcap` — the crash and
  pcap actions (Phase 3/4) must write there for the bundle to pick them up.
- **Dependencies** (all maintained in the last 12 months): `rusqlite`
  0.40.2 (MIT, `bundled` SQLite + `functions`), `csv` 1.4 (MIT/Unlicense),
  `zip` 8.6 (MIT, deflate only; 9.0 is still pre-release), `regex` 1.13
  (MIT/Apache, already in use), `tempfile` 3.27 (MIT/Apache, dev only).
Consequences: the UI queries a stored session by passing `session_id` in
`HistoryQuery`/`LogFilter`; new commands `list_sessions`, `open_session`,
`delete_session`, `store_stats`, and `export_data` now returns
`ExportResult`. `Engine::new` degrades to memory-only with an `Error`
notice when the store cannot open. Retention settings reach the UI in
6.6; a store-side `EngineError::Store` variant is a small follow-up (store
errors currently map onto `Device` errors with an `Io`/`Unsupported` code).

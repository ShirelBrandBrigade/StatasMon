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

## ADR-013 · 2026-09-09 · iOS tier 2: tunnel strategy, sysmontap mapping, cadences
Status: accepted (live check on the iPad pending; see docs/PLAN.md 4.1–4.6).
Context: Phase 4 needed the developer tunnel and the DVT loops built on
top of the tier-1 `IosSource` without changing tier-1 behaviour.
Decisions:
- **Tunnel.** `CoreDeviceProxy` is opened like any lockdown service (pair
  record, TLS), and the `idevice` crate's userspace TCP stack (`jktcp`,
  `tunnel_tcp_stack`) carries the tunnel's IPv6 packets in-process. No TUN
  device, no root, no OS permission, no extra port on the host on either
  OS. One RSD handshake reads the service table; each DVT loop opens its
  own `dtservicehub` connection so a hung service never blocks another.
  iOS 18.2's TCP + TLS-PSK change concerns the network (remoted) transport
  only; the USB CDTunnel framing is the same from 17.4 on, so no per-major
  branch. Developer Mode off is detected two ways: `amfi` at connect (and
  every 30 s while tier 1) and the RSD table lacking `dtservicehub`.
  Tunnel death = a transport error on any service open/read, or the
  heartbeat (one TCP connect to the RSD port every 15 s) failing; the
  supervisor then emits `DeviceState Tier1` + one `Error TunnelFailed`
  and retries with 5 s → 60 s backoff. `stop()` cancels the loops and
  closes the stack.
- **sysmontap attributes** are chosen at runtime: `deviceinfo.sysmon
  SystemAttributes`/`ProcessAttributes` ∩ our wanted lists (both the
  `vm*`/`net*`/`disk*` names iOS 16–17 advertise and the Instruments
  display names), falling back to the five system and seven process
  attributes every tool since iOS 13 uses. Interval = `fast_ms` (1 s);
  the process table (top 250 by CPU) is emitted every `medium_ms`.
- **Units and mapping** (to verify on each major with a captured
  fixture): `CPU_TotalLoad` is a percentage, system-wide and per core;
  `CPU_UserLoad`/`CPU_SystemLoad` of −1 mean "not split" and are not
  emitted; process `cpuUsage` is a fraction of one core, published ×100
  in the `top` convention of ADR-010; memory pages × `vmPageSize` (16 KiB
  default), `mem.app` = internal pages, `mem.cached` = external
  (file-backed) pages, `mem.wired`, `mem.compressed` = compressor pages,
  `mem.used` = app + wired + compressed (Activity Monitor's figure), else
  total − free; `mem.total` = `hardwareInformation` RAM, else sysmontap
  `physMemSize`, else tier-1 MobileGestalt; net/disk counters are
  cumulative since boot → rates from deltas (bytes×8 → `net.*_bps`),
  totals count from the first tick like Android.
- **Per-core kinds.** Apple silicon numbers efficiency cores first and
  performance cores last (`powermetrics` on M1: E-Cluster CPU 0–3,
  P-Cluster CPU 4–7), so with the idmap layout (A14 = 2P + 4E) cores
  0–3 are Efficiency and 4–5 Performance; `sysmontap::PERFORMANCE_CORES_
  LAST` is the single switch if a chip proves otherwise; a layout that
  does not fit the reported count yields `Unknown`. Frequencies stay
  dashed (iOS does not report them).
- **Networking.** The DVT networking service's connection-update events
  are cumulative per connection: per-pid throughput is the windowed
  delta (bits/s), joined into `ProcessRow.net_bps`; `net.connections` =
  connections updated in the last 10 s; `net.retransmits` = retransmitted
  segments in the window; `net.latency_ms` = mean of the reported
  `avg_rtt` (kernel TCP RTT, milliseconds — `RTT_DIVISOR` is the one
  knob), only when the service reported one; nothing is probed. It
  reports no RSSI, so `WifiDetails` stays unavailable at tier 2.
- **Not available at tier 2, with reasons:** load average (iOS has none
  for the host), a memory-pressure level (warnings come from the log),
  the front-most app (the developer services do not report it; the
  brief's "ForegroundApp available" was not honoured because it would
  fabricate a state), core frequencies, temperature sensors.
- **Energy gauge** is off by default (`IosSource::set_energy_enabled`)
  because it breaks on some iOS 18 builds; when on, the top five pids by
  CPU are sampled every `medium_ms` and `energy.cost` is bucketed low
  < 1 ≤ medium < 10 ≤ high (provisional thresholds until compared with
  Xcode's gauge); a failure withdraws `Capability::Energy` with a reason
  and nothing else changes.
- **Dependencies:** no new crates; `idevice` features `core_device_proxy`,
  `tunnel_tcp_stack` (pulls `jktcp`, MIT, same author), `xpc`, `dvt`
  (`ns-keyed-archive`, MIT), `sysmontap`, `network_monitor`,
  `device_info`, `energy_monitor`.
Consequences: `IosSource` publishes `Tier2` with the developer services'
capabilities and degrades to `Tier1` with a visible reason; the sheet
gains RAM, core count, uptime/boot time and the tunnel description;
fixtures under `fixtures/ios/dvt/` are shape-only until the iPad with
Developer Mode on is captured with `capture_dvt_fixture`.

## ADR-014 · 2026-09-09 · Settings file and what it can change at runtime
Status: accepted. Context: step 6.6 needed user settings (sampling rates,
retention, adb location, update-check opt-in, a reserved theme) that
survive restarts and reach the core. Decisions:
- **Shape.** `Settings { sampling: SamplingConfig, retention:
  { max_session_hours (4), keep_days (30), max_total_gb (none) }, adb_path
  (none), update_check (false), theme ("dark", reserved), version (1) }`
  lives in `crates/protocol` (`commands.rs`) so the UI and the core share
  one type; bindings are generated like every other protocol type.
- **File.** `settings.json` (pretty JSON) in the same folder as the
  session store: the app data folder, or `STATASMON_STORE_DIR` in dev runs.
  Written as `settings.json.<pid>.tmp` then renamed into place. A missing
  file means defaults; an unreadable, unparsable or out-of-range file means
  defaults plus a `tracing` warning (never a crash, never a silent partial
  merge). A file from a newer `version` is read with unknown fields
  dropped and restamped on the next save.
- **Validation** happens in the core (`src-tauri/src/settings.rs`) on
  every `set_settings`, and the UI mirrors the same bounds for inline
  messages: fast ≥ 250 ms, medium ≥ 1 000 ms, slow ≥ 5 000 ms, heavy 0 or
  ≥ 30 000 ms; hours per session 1–48; days to keep 1–365; the size cap,
  when set, a positive number of GB (1 GB = 1024³ bytes, matching how the
  UI formats bytes); `adb_path`, when set, must be an existing file; only
  the dark theme is accepted. `set_settings` answers with what it stored.
- **When settings take effect.** `lib.rs` loads the file before creating
  the engine and passes sampling into `EngineConfig` and retention into
  `StoreConfig`; an explicit `adb_path` builds the Android discovery with
  `AdbDiscovery::new(AdbServer::new(path))` instead of the locator. The
  engine's config and the store's retention are immutable after
  construction and the workspace forbids `unsafe` (so `STATASMON_ADB`
  cannot be set from inside the process), therefore saved values take
  effect **at the next launch**; the Settings screen says so on each
  section rather than implying a live change. Consequence: a small core
  follow-up (`Engine::set_sampling` for devices that start later,
  `Store::set_retention`, a re-locate hook on `AdbDiscovery`) would make
  the three live without changing the file or the UI.
- **Update check** is only a stored flag until Phase 7 ships the updater;
  the app makes no network call either way.

## ADR-015 · 2026-09-09 · Distribution channels and update policy
Status: accepted. Context: Phase 7.4 needed a way to ship installers for
macOS and Windows, to update them, and to do both without breaking the
"no telemetry, no network calls" rule. Decisions:
- **Channel.** Builds come only from the `release` GitHub Actions
  workflow on a `v*` tag: macOS `.dmg` + `.app` for `aarch64` and
  `x86_64`, Windows per-user NSIS `.exe` (`installMode: currentUser`, no
  admin prompt) and `.msi`, plus the updater artifacts and `latest.json`.
  CI uploads them to a **draft** GitHub release; a person smoke-tests both
  installers and publishes the draft (`docs/RELEASE.md` §6). No other
  distribution (no store listings, no third-party download sites) in v1.
- **Signing.** macOS builds are signed with a Developer ID Application
  certificate and notarized; Windows builds are signed through Azure
  Artifact Signing (preferred) or an OV/EV certificate. Until the secrets
  exist the same workflow produces unsigned installers so the pipeline is
  exercised, but **unsigned builds are never published as a release**; the
  CHANGELOG says so while it is the case. Adding the secrets is the whole
  remaining step; nothing in the code changes.
- **Updater.** `tauri-plugin-updater` with a static `latest.json` on the
  latest published release (`https://github.com/<owner>/StatasMon/releases/latest/download/latest.json`),
  verified with a minisign key pair: the public key is in
  `tauri.conf.json`, the private key lives outside the repository
  (`~/.tauri/statasmon.key`, the team password manager, and the
  `TAURI_SIGNING_PRIVATE_KEY` secret). Artifacts whose signature does not
  verify are rejected by the plugin; there is no way to turn that off.
- **Policy.** The check is **opt-in** (Settings → "Check for updates on
  launch", default off, ADR-014) and runs once per launch. A found update
  becomes a notice ("Update available: vX (…)"); nothing is downloaded or
  installed until the user invokes `install_update` (the button is a
  Phase 8 UI step). Both commands answer with a clear "updater not
  configured" error when the endpoint still holds the `OWNER` placeholder
  or the public key is empty, instead of a failed request. Windows installs
  in `passive` mode (progress bar, no questions) and relaunches; macOS
  swaps the `.app` in place and relaunches via `tauri-plugin-process`.
- **No telemetry, restated.** The update check is the only network
  request the app makes on its own, it is off by default, and it sends
  nothing but the plain GET for `latest.json` (the plugin adds no
  identifiers). Device data, serials, logs and settings never leave the
  user's app-data folder unless the user exports them. Crash reporting,
  usage analytics and download counters stay out of scope; a change to
  this needs its own ADR and the user's decision.
- **Versions.** One version string across `Cargo.toml` (workspace),
  `tauri.conf.json` and the package manifests, moved by
  `scripts/bump-version.sh`; the tag is `v<version>`. `CHANGELOG.md`
  follows Keep a Changelog and is the release body's source of truth.
Consequences: releases are reproducible from a tag; a lost private key
means one manual reinstall for every user (documented); the draft step
adds a human in the loop by design.

## ADR-016 · 2026-09-10 · The header shows the device name; the headline is a pill
Status: accepted. Context: hands-on QA with the live iPad showed the
headline sentence ("iPad (10th generation): com.apple.xpc.launchd[1]<…>
was closed…") changing several times a minute, wrapping to two lines and
reflowing every card below it; the user asked for the device name alone
at the top. Decisions:
- **`TopBar` renders the device's own name** (`DeviceSummary.name`, model
  name as fallback) as the `h1`, one fixed line with an ellipsis, and a
  one-line subtitle `model · link` that never changes during a session.
- **The headline becomes one pill** in the status row: a two-or-three-word
  label per rule (`RULE_LABEL` in `TopBar.tsx`, e.g. "App crashed",
  "Low memory", "Running hot"), toned by severity, hidden while the
  severity is nominal. The full sentence and cause are the pill's tooltip.
  The status row is one non-wrapping line with a minimum height, so the
  header keeps one height whatever the device reports.
- **Names are shortened everywhere they are displayed** (`shortName` in
  `packages/protocol`, `display_name` in `crates/core/src/rules.rs`): the
  `<…>` wrapper and `[pid]` suffix go, and a reverse-DNS id keeps its last
  component. The raw identifier stays in the `title` attribute.
- **Chart marker chips never overlap**: a chip that would collide with one
  already drawn is skipped (its line is still drawn) and labels are cut
  at 140 px with an ellipsis (`drawMarkers` in `TimeSeriesChart.tsx`).
- **iOS log admission defaults to Notice at 200 lines/s** (was Info at
  500): Notice is os_log's "Default" level, so an app's ordinary logging
  still arrives while the ~4 000 lines/s of system Info/Debug chatter
  does not. `STATASMON_IOS_LOG_MIN_LEVEL=info|debug` widens it.
Consequences: the rules engine and `Headline` contract are unchanged
(the sentence is still stored, exported and used by the sidebar dot);
`docs/PLAN.md` 1.4 "headline sentence in the header" is superseded by
this ADR; the Overview e2e checks the `h1` for the device name.

## ADR-017 · 2026-09-10 · Chart smoothing, the always-visible range control, and the Device sheet layout
Status: accepted. Context: after the first hands-on QA the user asked for
more polished charts, for the Live range control to stay reachable at all
times, and for a cleaner Device page. Decisions:
- **Charts draw monotone cubic curves** (Fritsch–Carlson, `monotonePaths`
  in `TimeSeriesChart.tsx`): smooth between samples, never overshooting a
  sample, so a bounded metric (CPU 0–100 %) never rings past its limits.
  `null` still breaks the line (a real gap); `undefined` (another series'
  tick) is run through. The fill closes to the zero line and each line
  has a soft halo. **A window is never wider than its data plus a 30 s
  floor** (`MIN_SPAN_S`): a fresh session fills the plot from the left
  instead of squeezing thirty seconds into the right edge of a five-minute
  grid, and grows until the window is full. Vertical grid lines are dashed
  and lighter, ticks are spaced wider (x 96 px, y 40 px), Performance
  charts are taller, and an empty chart says "Waiting for samples…"
  instead of showing a bare grid. Data still arrives through `setData`.
- **The range control is part of every per-device screen.** `TopBar`
  (device name, model · link, status pills, Live · 5 min · 15 min · 1 h ·
  Session, Export) now also heads the Device sheet, and the header wraps its
  tools under the name in a narrow window instead of clipping them.
  Sessions and Settings are not device screens and keep no range control.
  "Live" is a 60 s window and stays selectable whether or not the session
  is recording; a paused session simply stops appending.
- **Device sheet layout**: the shared header, then a hero card (device
  tile, model name, marketing name, platform pill, link, id, Refresh) with
  a seven-figure spec strip (system, chip, cores, memory, storage, battery
  design capacity, uptime), then Identity / Software / Hardware / State /
  Connection / Capabilities in two columns and the raw keys last.
  Capabilities show as chips for what is available and as name + reason
  rows for what is not. The two columns are a multi-column flow with two
  fields per row inside each card, so no card leaves a hole under a
  shorter neighbour. Every missing value is still a dash with the
  "Not reported by the device" reason.
- **Log filtering by app.** The per-app tab row (Logs) and pill row
  (Overview) no longer list the noisiest processes: they show "All", the
  app under test, running apps that have logged, and whatever is selected.
  An app picker ("+ app", `AppPicker.tsx`) lists running apps first
  (name, bundle id, lines logged) and the system processes after, with a
  search box; a click selects that one app alone, Shift-click adds it,
  Escape or a click outside closes. The filter store is unchanged.
- **Scrollbars follow the palette**: 8 px, rounded, `--scroll-thumb` on a
  transparent track, set once in `styles.css` through the WebKit/WebView2
  `::-webkit-scrollbar` pseudo-elements only. The standard
  `scrollbar-width`/`scrollbar-color` properties are deliberately not
  set: when they are, WebKit ignores the pseudo-elements and draws its own
  flat bar, which is what looked out of place.
Consequences: chart tests that compare pixel paths must expect curves;
the Device tests target the hero (`device-hero`, `device-specs`) and the
two capability lists; the `h1` on every device screen is the device name;
the log tests select apps through the picker.

## ADR-018 · 2026-09-15 · Always live, app-drawn scrollbars, and motion
Status: accepted. Context: the user wants the screen to show live data
whenever a device is connected (recording only decides what is kept), the
Network chart went blank after the networking service dropped, the native
scrollbars looked foreign, and the UI had no motion at all. Decisions:
- **Every provider message is forwarded and kept in memory whatever the
  session status.** `Engine::handle` pushes `Samples`, `Cores`,
  `Processes` and `Log` into the rings and to the bus while Idle, Paused or
  Stopped; only counting and store writes require Recording. The only time
  the app shows nothing is when nothing is connected.
- **The networking loop releases the device totals** (`net_from_networking
  = false`) on every exit, so sysmontap's totals resume and the Network
  chart never goes blank after a service failure or a tunnel restart.
- **Scrollbars are drawn by the app** (`shell/overlayScrollbars.ts`): the
  native bars are hidden on every managed container and a 6 px rounded
  thumb on `--scroll-thumb` is placed over it, visible while scrolling or
  hovering, fading after 900 ms, draggable, both axes. One installer
  watches the DOM, so the page, the log lists (Virtuoso), tables, the
  picker and the session panels all get the same bar on macOS and Windows.
  The `::-webkit-scrollbar` rules stay only as a fallback for containers
  the installer does not know.
- **Motion** (`motion.css`): controls answer hover and press in 120 ms
  (`--ease-out`); rings, core bars, stack segments and top-app bars glide
  to the next reading in 320 ms; screens fade-rise in 200 ms with cards
  staggered 30 ms; popovers and dialogs pop in 160 ms; the recording dot
  breathes. Charts still append without easing and numbers never animate.
  Everything is off under `prefers-reduced-motion`.
Consequences: the engine test asserts that samples are forwarded while
paused; the log follow-tail control moved to the app tab row; the docs
for `statasmon_core` describe the new policy.

## ADR-019 · 2026-09-15 · Packed card layouts and continuous motion
Status: accepted. Context: the Device and Settings pages still left holes
beside short cards and their fields did not line up; page and tab changes
were cuts. Decisions:
- **`Masonry` (`shell/Masonry.tsx`)** packs cards by placing each one under
  the shortest column so far, in document order, on a 2 px CSS grid with
  explicit column and row spans measured by a ResizeObserver; columns come
  from the available width (`minColumnWidth`, `maxColumns`). The Device
  sheet and Settings use it. Inside a Device card the fields sit on one
  grid (label · value · label · value) so labels and values align down the
  whole card; one field per row under 1320 px.
- **Sliding highlights** (`positionThumb` in `packages/ui`): the range
  segment, the sidebar nav and the log app tabs draw one thumb that glides
  to the active item (260–280 ms) instead of restyling the item.
- **Page changes crossfade** through the View Transitions API when the
  webview has it (WKWebView on macOS 15+, WebView2): the old page fades up
  and out in 140 ms, the new one fades in from 8 px below in 240 ms, the
  sidebar is excluded. Without the API the page still fade-rises.
- **Everything that appears or changes subject moves**: dialog scrims
  fade under the popping dialog, the session and app detail panels re-enter
  when their subject changes (keyed remounts), chart tooltips fade instead
  of vanishing, the primary button lifts 1 px on hover.
Consequences: all of it is off under `prefers-reduced-motion`; the
Masonry container is `data-testid="masonry"` for tests.

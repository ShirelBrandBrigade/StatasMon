# StatasMon — development plan

Status line (keep current): **Phase 0 in progress (0.3–0.6) · 2026-09-09 · plan v1.1**

This is the working document for the lead session and every sub-agent. It
says what we are building, what the platforms can actually provide, the
architecture, the repository conventions, the phases with their steps and
acceptance criteria, and how work is split between agents. Read
`CLAUDE.md` first, then this file, then `docs/DESIGN-SYSTEM.md`,
`docs/FEATURES.md` and the three research reports in `docs/research/`.

---

## 1. Product

**One sentence.** A macOS + Windows desktop app that connects to one or
more iPhones, iPads and Android devices at once, records everything they
report every second (CPU per core, memory, thermal, battery, storage,
network, processes, logs, crashes, full device information), and shows it
in one dark glass dashboard with per-app views and exports.

**Primary user.** A developer or QA engineer testing their own app on real
devices who wants to see, at a glance, what the device is doing and why,
then dig into the log and export evidence.

**v1 scope.** Everything in `docs/FEATURES.md` sections 0–11 for iOS 17.4+
and Android 12+, USB first, Wi-Fi where the platform makes it cheap.
The app is **StatasMon** (bundle id `com.statasmon.app` until the user says
otherwise); the sample app in mockups is **FieldKit**, a placeholder.

**Non-goals for v1.** Root/jailbreak features, remote control of the
device, screen mirroring, cloud sync, accounts, plugins, iOS 16 and lower,
Android 11 and lower, Linux desktop (kept compilable, not shipped).

---

## 2. Platform reality (what the research settled)

Full detail with sources: `docs/research/ios.md`, `docs/research/android.md`.
The UI shows a dash and a reason for anything a platform cannot provide.

| Item | iOS / iPadOS | Android |
|---|---|---|
| Device info | lockdown `GetValue` (ProductType, ProductVersion, BuildVersion, HardwarePlatform, ChipID, CPUArchitecture, HardwareModel, Serial, UDID, name, model number, region, Wi-Fi/BT MAC), RAM via DVT `deviceinfo.hardwareInformation`; marketing name and chip name from a bundled AppleDB-derived table | `getprop` (model, manufacturer, codename, release, sdk, build id, fingerprint, security patch, `ro.soc.*`, abilist), `/proc/meminfo`, `/proc/version`, `/proc/uptime`, `settings get global` |
| CPU total + per core | DVT sysmontap `SystemCPUUsage` + `PerCPUUsage` (needs Developer Mode + tunnel) | `/proc/stat` deltas per `cpuN`; frequencies from cpufreq sysfs |
| Memory | sysmontap free/active/inactive/wired/compressor/swap; memory warnings from os_trace | `/proc/meminfo`; PSS via `dumpsys meminfo` (slow, 60 s+); PSI **not readable** → proxy from `/proc/vmstat`; LMK kills from logcat |
| Thermal state | **No query API.** Derived from `thermalmonitord` lines in os_trace (Nominal/Fair/Serious/Critical); unknown until first transition, shown as "not reported yet" | `dumpsys thermalservice` status 0–6 + HAL sensor list (10–30 s) |
| Temperatures | Battery only (`AppleSmartBattery.Temperature` via diagnostics_relay) | HAL sensors from thermalservice; battery from `dumpsys battery` |
| Battery | lockdown `com.apple.mobile.battery` (%, charging) + IORegistry `AppleSmartBattery` (voltage, instant amperage, cycle count, design/full capacity, temperature) — no tunnel needed | `dumpsys battery` + `/sys/class/power_supply/battery/*` (current_now, cycle_count, charge_full, charge_full_design) |
| Storage | lockdown `com.apple.disk_usage`; per app via installation_proxy `StaticDiskUsage`/`DynamicDiskUsage`; no photo/system split | `df`, `dumpsys diskstats` (categories, may be stale), `pm list packages -f` + `stat` |
| Network throughput / packets | sysmontap `NetBytesIn/Out`, `NetPacketsIn/Out`; per-process and RTT via DVT networking service | `/proc/net/dev`; per-UID via `dumpsys netstats detail --poll` (10–30 s); Wi-Fi via `cmd wifi status` |
| Packet capture | `com.apple.pcapd` (no tunnel) → .pcap | **not available** without root in v1 |
| Processes | sysmontap process attributes (cpuUsage, physFootprint, memResidentSize, threadCount) | `/proc/<pid>/stat` deltas + `ps`/`top` snapshots; pid↔package via `pm list packages -U` and `/proc/<pid>/cmdline` |
| Logs | `com.apple.os_trace_relay` structured stream (pid, process, subsystem, category, level) — no tunnel | `logcat -v threadtime -v epoch -v uid -v usec -b main,system,crash,events`, `--pid` filters |
| Crashes | crashreportmover + AFC copy of `.ips` reports | `dumpsys dropbox --print` (data_app_crash, data_app_anr); bugreport on demand |
| Energy per process | DVT energy gauge (fragile across iOS majors; optional) | — (batterystats on demand only) |
| Connection | usbmuxd (macOS built in; Windows needs Apple Mobile Device Support from iTunes/"Apple Devices"); iOS 17.4+ userspace tunnel, no root; iOS 18.2+ TCP+TLS-PSK | adb server on 5037 (reuse) or our own port; USB needs OEM driver on Windows; wireless via `adb pair`/`connect` |

Hard constraints that shape the design:
1. On iOS, everything under "DVT" needs **Developer Mode on** and a tunnel.
   Battery, storage, device info, logs, crashes and pcap work without it.
   The app therefore has two iOS tiers and must say which one is active.
2. `pymobiledevice3` is GPL-3 and cannot ship inside the app; the Rust
   `idevice` crate (MIT) is the primary iOS library, `go-ios` (MIT) is a
   fallback sidecar only if `idevice` fails on a given iOS version.
3. The prebuilt `adb` cannot be redistributed under the SDK license. v1
   detects an installed adb, otherwise downloads platform-tools on first run
   after the user accepts Google's terms (scrcpy's approach). Building adb
   from AOSP is the later "clean" option (ADR when decided).
4. Windows needs third-party drivers for both platforms; the app must
   detect their absence and show exact instructions instead of failing.
5. Sampling is 1 Hz for cheap sources; expensive `dumpsys`/DVT snapshots run
   at their own slower cadence and the UI shows the age of every value.

---

## 3. Architecture

### 3.1 Stack (versions from `docs/research/stack.md`; pin at bootstrap)
- Shell: **Tauri 2.11** (plugins: shell, dialog, fs, log, updater, opener).
  Window is opaque; never `transparent: true`.
- Core: **Rust** (edition 2024, stable toolchain), tokio, `idevice` (iOS),
  `adb_client` server-proxy mode (Android), `rusqlite` bundled with WAL,
  `serde`/`serde_json`, `ts-rs` for TypeScript type generation, `csv`,
  `zip`, `tracing`.
- UI: **React 19 + TypeScript 5 + React Compiler**, Vite 8, Zustand 5,
  Tailwind v4 with our tokens, Radix primitives, **uPlot** for time series,
  hand-rolled SVG rings/meters, **react-virtuoso** for the log.
- Fonts bundled via `@fontsource-variable/manrope` and
  `@fontsource-variable/jetbrains-mono`. No runtime font fetching.
- Tooling: pnpm workspaces + Cargo workspace, Biome, Vitest, Playwright,
  `cargo clippy -D warnings`, `cargo test`, GitHub Actions matrix
  (macos-latest arm64 + x86_64, windows-latest) with `tauri-action`.

### 3.2 Process model
One Tauri process. The Rust core runs providers on tokio tasks; the
webview renders. Sidecars: the `adb` binary (spawned as a server only when
none is running); optionally a `go-ios` binary in Phase 4 as a fallback.
No Python anywhere in the shipped app.

### 3.3 Crates and packages
```
devicelog/
  apps/desktop/              React app (Vite) — the only webview
  packages/ui/               tokens.css, primitives (Card, Ring, StackBar, Pill…)
  packages/protocol/         GENERATED TypeScript types (from crates/protocol)
  crates/protocol/           Rust types for every message; ts-rs export; metric catalog
  crates/core/               session engine: scheduler, bus, ring buffers, headline rules
  crates/store/              SQLite schema, migrations, writer, queries, retention
  crates/export/             .log / .jsonl / .csv / .zip writers
  crates/device/             DeviceSource trait, Capability set, discovery, common parsers
  crates/device-fake/        deterministic + replay source
  crates/device-android/     adb provider
  crates/device-ios/         idevice provider (lockdown tier + DVT tier)
  crates/idmap/              ProductType/ChipID/ro.soc → marketing names (bundled JSON)
  src-tauri/                 Tauri app: commands, channels, state, plugins
  fixtures/                  recorded device outputs used by tests and the fake
  docs/                      this plan, design system, features, research, ADRs
  scripts/                   dev helpers (mock run, fixture capture, release)
```

### 3.4 The DeviceSource contract (crates/device)
```rust
pub trait DeviceSource: Send + Sync {
    fn id(&self) -> DeviceId;
    fn platform(&self) -> Platform;            // Ios | Android | Fake
    fn capabilities(&self) -> CapabilitySet;   // what this device/tier can provide, with reasons for gaps
    async fn info(&self) -> Result<DeviceInfo>;
    async fn start(&self, sink: EventSink, cfg: SamplingConfig) -> Result<()>;
    async fn stop(&self) -> Result<()>;
    async fn run_action(&self, action: DeviceAction) -> Result<ActionResult>; // pcap start/stop, fetch crash, bugreport…
}
```
`CapabilitySet` is a map `Capability → Available | Unavailable(reason)`
(e.g. `CpuPerCore → Unavailable("Developer Mode is off on this iPhone")`).
The UI reads it to draw dashes and tooltips. Capabilities are re-evaluated
on reconnect and when Developer Mode changes.

### 3.5 Events (crates/protocol) — the one data contract
- `DeviceAttached { device, transport }`, `DeviceDetached`, `DeviceState { pairing, tunnel, tier }`
- `Info { device_id, info: DeviceInfo }` (full document, re-sent on change)
- `Samples { device_id, ts_ms, values: Vec<(MetricId, f64)> }` — one per tick
- `Cores { device_id, ts_ms, per_core: Vec<CoreSample> }`
- `Processes { device_id, ts_ms, rows: Vec<ProcessRow> }` (cadence 2–5 s)
- `Log { device_id, entries: Vec<LogEntry> }` (batched, ≤ 500 per message)
- `Event { device_id, ts_ms, kind: ThermalChange | MemoryWarning | LmkKill | Crash | Anr | Mark | Connection, detail }`
- `Capabilities { device_id, set }`
- `Progress { task_id, pct, message }` and `Error { device_id?, code, message, hint }`

`MetricId` is a closed enum in `crates/protocol/src/metrics.rs` with unit
and display metadata: `cpu.total`, `cpu.core.N` (via Cores), `cpu.load1`,
`mem.total`, `mem.used`, `mem.free`, `mem.app`, `mem.wired`,
`mem.compressed`, `mem.cached`, `mem.swap`, `mem.pressure` (0–3),
`thermal.state` (0–3 iOS / 0–6 Android, mapped to a common 0–3 severity
plus the raw), `temp.battery`, `temp.soc`, `temp.skin`, `temp.<sensor>`,
`bat.percent`, `bat.voltage`, `bat.current`, `bat.power`, `bat.temp`,
`bat.cycles`, `bat.health`, `bat.charging` (0/1), `disk.total`,
`disk.used`, `disk.free`, `disk.apps`, `disk.media`, `disk.system`,
`disk.other`, `net.rx_bps`, `net.tx_bps`, `net.rx_pps`, `net.tx_pps`,
`net.rx_total`, `net.tx_total`, `net.rssi`, `net.link_mbps`,
`net.connections`, `net.retransmits`, `proc.count`. Adding a metric = one
enum variant + catalog entry + fake generator + docs row.

### 3.6 Data flow
Provider task → `EventSink` (mpsc) → core bus → three consumers:
1. **Ring buffers** per device (typed arrays; 3,600 samples per metric,
   50,000 log lines) for the live UI.
2. **Store writer** (single thread, batches per second in one transaction).
3. **Rules engine** producing the headline sentence and status pills.
The Tauri layer subscribes and pushes to the webview over **Channels**:
one `metrics` channel (1 message per second per device, all metrics),
one `logs` channel (pages), one `events` channel. The webview keeps its
own mirror buffers outside React; uPlot and Virtuoso read them directly;
React re-renders only headline, pills, tables at ≤ 2 Hz.
Commands from UI → core: `list_devices`, `connect`, `disconnect`,
`start_session`, `pause`, `resume`, `stop`, `mark`, `set_range`,
`query_history(metric, from, to, step)`, `query_logs(filter, page)`,
`export(scope, format, path)`, `device_action`.

### 3.7 iOS connection state machine
`Detected (usbmuxd)` → `Paired?` (prompt "Trust This Computer" guidance) →
`Tier 1 ready` (lockdown, diagnostics, os_trace, pcapd, crash reports) →
`Developer Mode?` (if off: show how to enable, stay tier 1) →
`Tunnel` (CoreDeviceProxy userspace TCP stack; iOS 18.2+ TLS-PSK) →
`RSD handshake` → `Tier 2 ready` (DVT sysmontap, networking, deviceinfo,
energy optional). Any tier-2 failure degrades to tier 1 with a visible
reason, never a crash. Reconnect with backoff on detach.

### 3.8 Android connection
Find `adb` (bundled-fetched or installed) → find or start server → track
devices (`host:track-devices`) → per device: probe once (which sysfs/proc
paths are readable, thermal HAL present, cycle_count present) → build
`CapabilitySet` → open persistent shells: one for the 1 s batch, one for
2–5 s, one for slow dumpsys, one `logcat` stream. Wireless: `pair` +
`connect` flow in the Add-device dialog.

### 3.9 Storage (crates/store)
SQLite per app-data dir, one file per session (`sessions/<id>.sqlite`) plus
an index db. Tables: `session`, `device_info` (JSON document, versioned),
`samples(ts_ms, metric_id, value)` indexed `(metric_id, ts_ms)`,
`cores(ts_ms, core, value)`, `processes(ts_ms, pid, name, cpu, mem, threads,
net)`, `logs(seq, ts_ms, pid, process, level, tag, subsystem, message)` +
FTS5 on `message`, `events(ts_ms, kind, detail_json)`, `marks`. Retention:
configurable cap per session (default 4 h at 1 Hz) and days to keep
(default 30). Writes never touch the UI thread; reads for history charts
downsample in SQL (`step` parameter).

### 3.10 Export (crates/export)
`.log` (one line per entry, iOS/Android-neutral format), `.jsonl` (full
fields), `.csv` (metrics wide or long), `.zip` session bundle (device
info, metrics CSV, logs JSONL, events, marks, crash reports, pcap).
Scope: current filter / selection / time range / session / one app.
Runs on a worker, reports progress, returns the path.

### 3.11 Headline rules (crates/core/rules.rs)
Ordered, first match wins, with cause sentence:
critical thermal → serious thermal → app crashed/ANR in last 60 s →
memory pressure/LMK kill → fair/light thermal → battery hot (> 40 °C) →
high retransmits/dropped → developer mode off (iOS) → "Everything looks
normal". Each rule names the app or process when it can.

### 3.12 UI structure (apps/desktop)
Routes: Overview (the AD-1 mockup), Performance, Network, Apps &
processes (+ app detail), Logs, Sessions, Device, Settings. Sidebar with
DeviceCards (one per connected device, each with its own recording state)
and the RecordingCard for the active device. Range segment is global.
Everything reads `packages/ui/tokens.css`; components listed in
`docs/DESIGN-SYSTEM.md`.

---

## 4. Conventions
- Branches `phase<N>/<step>-<slug>`; Conventional Commits; one PR per step.
- Rust: `cargo fmt`, `cargo clippy --all-targets -D warnings`, no `unwrap`
  outside tests, errors via `thiserror` with user-facing `hint`.
- TS: strict, Biome, no `any`, components are functions, hooks in
  `src/hooks`, no inline hex colors (lint rule), numbers formatted through
  `formatValue(metricId, value)` so units are consistent.
- Every provider parser is a pure function `parse_x(&str) -> Result<T>`
  with a fixture test; fixtures are captured from real devices with
  `scripts/capture-fixture` and stored under `fixtures/<platform>/<os>/`.
- Every screen has a Playwright test against the fake device.
- Perf budgets (enforced in CI where possible): UI idle CPU < 3% with two
  devices connected; 1 Hz update ≤ 4 ms main-thread work; log ingest
  10,000 lines/min without frame drops; memory < 300 MB after 1 h.
- Accessibility: keyboard reachable, focus visible, status never color-only.

Commands (defined in Phase 0): `pnpm i`, `pnpm dev` (real devices),
`pnpm dev:mock` (fake device), `pnpm check` (biome + tsc + vitest),
`pnpm e2e`, `cargo check --workspace`, `cargo test --workspace`,
`pnpm tauri build`.

---

## 5. Phases and steps

Each step: deliverable · owner role · verification. Mark done with a date.

### Phase 0 — Bootstrap (foundation everyone builds on)
0.1 Repo, workspaces, toolchains pinned, Biome/clippy configs, CI matrix · done 2026-09-09
    building an empty Tauri app on macOS and Windows · release · CI green.
0.2 `crates/protocol`: events, `MetricId` catalog with units, `DeviceInfo` · done 2026-09-09
    document, `CapabilitySet`; `ts-rs` generation into `packages/protocol`
    · protocol · `cargo test` + generated TS compiles.
0.3 `crates/device` trait + `crates/device-fake` (seeded generator for every
    metric, scripted scenarios: thermal rise, memory warning, crash, app
    launch; replay of fixtures) · core-store/qa · fake runs headless and
    emits all event kinds.
0.4 `crates/core`: scheduler, bus, ring buffers, rules engine v1 · core ·
    unit tests for buffers and rules.
0.5 `src-tauri`: commands + channels wired to core; `pnpm dev:mock` shows
    events in a debug panel · core + ui · manual + Vitest on the client
    buffer mirror.
0.6 `packages/ui`: tokens from DESIGN-SYSTEM.md, fonts bundled, primitives
    (Card, Ring, StackBar, Legend, Pill, KeyValue, Sparkline, Button,
    Segment) with a Storybook-free gallery route · ui · visual check
    against the AD-1 mockup.
Acceptance: fake device streams into a token-styled placeholder screen on
both OSes from CI artifacts.

### Phase 1 — Overview screen on the fake device
1.1 App shell: sidebar with multi-device list, nav, RecordingCard,
    headline, range segment, export button · ui.
1.2 Metric cards: CPU (ring + core bars), Memory (ring + stack), Storage,
    Battery · ui.
1.3 Network card with uPlot area chart (down/up), packets, totals ·
    ui · 3,600-point stream at 1 Hz under 4 ms/frame.
1.4 Thermal and Top apps cards · ui.
1.5 Log card: Virtuoso, follow-tail, level and app filters, fault
    highlight, Rust-side filtering and paging · ui + core.
1.6 Multi-device: three fake devices at once, switching, per-device
    recording state, detach/reattach · core + ui.
1.7 Playwright suite for the Overview · qa.
Acceptance: the Overview matches AD-1 on the fake device; perf budgets met.

### Phase 2 — Android provider (real hardware first, because it is simplest)
2.1 adb locator: installed adb, `~/Downloads/platform-tools` for dev,
    first-run download with terms acceptance; server reuse/own port ·
    core-android · unit tests with a stub server.
2.2 Device tracking, probe, `CapabilitySet`, `DeviceInfo` (all getprop and
    proc fields, SoC name mapping) · core-android · fixtures from the
    user's Android device.
2.3 1 s batch sampler: `/proc/stat` per core, cpufreq, meminfo, loadavg,
    `/proc/net/dev`; parsers with fixture tests · core-android.
2.4 2–5 s sampler: `dumpsys battery` + power_supply sysfs, process
    snapshot with `/proc/<pid>/stat` deltas, `cmd wifi status` ·
    core-android.
2.5 Slow sampler: thermalservice, netstats per UID, diskstats, meminfo PSS
    (opt-in), foreground app · core-android.
2.6 logcat stream → `LogEntry`, `--pid` per-app filter, LMK kill and crash
    events, dropbox crash/ANR fetch · core-android.
2.7 Wireless pairing dialog (`adb pair`/`connect`, mDNS list) · ui + core.
Acceptance: the Overview runs live on a real Android device with every
FEATURES.md field either filled or dashed with a reason.

### Phase 3 — iOS tier 1 (no tunnel)
3.1 usbmuxd discovery on macOS and Windows (Apple Mobile Device Support
    detection + instructions), pairing/trust flow · core-ios.
3.2 `DeviceInfo` from lockdown `GetValue` (all keys), `crates/idmap`
    table (ProductType → marketing name, ChipID/HardwarePlatform → chip
    name, from AppleDB snapshot with a refresh script) · core-ios + idmap.
3.3 Battery: `com.apple.mobile.battery` + IORegistry `AppleSmartBattery`
    parser (voltage, amperage sign, temperature ×0.01, cycles, capacities,
    health %) at 2 s · core-ios.
3.4 Storage: `com.apple.disk_usage` + installation_proxy per-app sizes ·
    core-ios.
3.5 `os_trace_relay` stream → `LogEntry` (pid, process, subsystem,
    category, level), thermalmonitord transition parser → `thermal.state`
    and ThermalChange events; memory warning lines → events · core-ios.
3.6 Crash reports (mover + copy, parse `.ips` header into an event) ·
    core-ios.
3.7 pcapd start/stop to `.pcap` · core-ios.
Acceptance: iPhone and iPad show info, battery, storage, logs, crashes and
capture on macOS and Windows; CPU/memory/processes are dashed with the
Developer Mode reason.

### Phase 4 — iOS tier 2 (tunnel + DVT)
4.1 CoreDeviceProxy userspace tunnel + RSD with `idevice` (`tunnel_tcp_stack`),
    iOS 17.4–26 matrix in fixtures; TLS-PSK for 18.2+ · core-ios.
4.2 Developer Mode detection (`amfi`) and enable guidance · core-ios + ui.
4.3 sysmontap: request system attributes discovered at runtime
    (`PerCPUUsage`, memory, net counters) and process attributes; parsers
    with fixtures per iOS major · core-ios.
4.4 DVT networking service: interfaces, per-pid connections, RTT,
    retransmits · core-ios.
4.5 `deviceinfo.hardwareInformation` (RAM, cores), `machTimeInfo` uptime ·
    core-ios.
4.6 Optional energy gauge behind a capability flag · core-ios.
4.7 Fallback decision: if `idevice` fails on a supported iOS, evaluate
    go-ios sidecar (ADR) · lead.
Acceptance: full Overview on a real iPhone with Developer Mode on; tier
degrades cleanly when it is off.

### Phase 5 — Sessions, store, export
5.1 Store schema + migrations + writer + retention · core-store.
5.2 History queries with downsampling; range segment drives charts from
    the store beyond the live buffer · core-store + ui.
5.3 Marks (keyboard M), events on charts · core + ui.
5.4 Exports: `.log`, `.jsonl`, `.csv`, session `.zip`; scopes; progress ·
    export + ui.
5.5 Sessions screen: list, reopen, delete, compare two sessions (same
    metric overlaid) · ui.
Acceptance: record 1 h on two devices, reopen, export bundle, sizes and
timings within budget.

### Phase 6 — Remaining screens and per-app views
6.1 Performance screen (per-core detail, frequencies, memory breakdown
    history, thermal history, battery history) · ui.
6.2 Network screen (interfaces, per-process table, capture controls,
    Wi-Fi details) · ui.
6.3 Apps & processes screen + app detail (metrics + that app's log) · ui.
6.4 Logs screen (full height, regex, time range, buffers, per-app tabs,
    crash/ANR attachments) · ui.
6.5 Device screen (every DeviceInfo field grouped: identity, hardware,
    software, connection, capabilities with reasons) · ui.
6.6 Settings (adb location, sampling rates, retention, theme reserved,
    update check opt-in) · ui + core.
Acceptance: every FEATURES.md item has a home; Playwright covers each screen.

### Phase 7 — Hardening and release
7.1 Perf pass against budgets (two devices, 1 h) · qa.
7.2 Error surfaces: driver missing, untrusted device, tunnel failure, adb
    conflict, device unplugged mid-session · ui + core.
7.3 Windows pass on a real Windows machine (WebView2 quirks, drivers,
    fonts, blur) · release.
7.4 Signing + notarization, updater with static JSON, installers (dmg, msi
    or nsis) · release.
7.5 Docs: README, user guide (enable Developer Mode, install drivers),
    CHANGELOG · lead.
Acceptance: signed builds for both OSes install and run on clean machines.

---

## 6. Testing strategy
- Unit: every parser and rule (Rust), every formatter and store mirror (TS).
- Fixture replay: each provider replays captured outputs through the full
  pipeline; snapshot the resulting events.
- Contract: generated TS types are diffed against the Rust source in CI;
  a mismatch fails the build.
- UI: Vitest for components, Playwright for screens on `pnpm dev:mock`.
- Perf: a `scripts/perf` run streams 3 fake devices for 10 min and asserts
  the budgets.
- Manual device matrix (kept in `docs/DEVICE-MATRIX.md` from Phase 2):
  iPhone (latest iOS, one older), iPad, Android phone, Android tablet; on
  macOS and Windows.

## 7. Risks and mitigations
| Risk | Mitigation |
|---|---|
| `idevice` breaks on a new iOS | fixtures per iOS major; go-ios fallback sidecar; tier-1 always works |
| iOS thermal has no API | derive from logs, label as "reported by device log", show time since last transition |
| adb licensing | detect/download-with-consent in v1; AOSP build task tracked as ADR |
| Windows drivers | detection + instructions screens; Windows test machine required by Phase 3 |
| IPC throughput | batching, channels, typed-array mirrors; measure in Phase 1.3 |
| Frosted blur cost | few large cards, opaque window, measure GPU on macOS |
| Scope creep | FEATURES.md is the list; additions go through the user and an ADR |

## 8. Sub-agent playbook
Task template (`docs/AGENT-TASK-TEMPLATE.md`): Task · Phase/Step · Files
allowed · Inputs (docs, fixtures) · Definition of done · Verification
commands · Report format.

Parallelization by phase:
- Phase 0: release (0.1) → then protocol (0.2) → in parallel: core (0.4),
  qa/fake (0.3), ui tokens (0.6); then 0.5 by lead.
- Phase 1: ui steps 1.1–1.5 in parallel by feature area (disjoint
  component folders); core for 1.5/1.6; qa for 1.7.
- Phase 2 and 3 can run in parallel (different crates) once Phase 1.1 exists.
- Phase 4 after 3; Phase 5 after 1; Phase 6 after 5; Phase 7 last.
Review checklist for the lead: matches step · files inside allowance ·
tests added and passing (output pasted) · fixtures added for parsers ·
no literal colors · capabilities honored (dashes with reasons) · docs
touched · ADR if a decision changed.

## 9. Answered and open questions
Answered 2026-09-09: name **StatasMon**; repository at
`/Users/bb/Desktop/StatasMon` (git, branch `main`); test devices in
`docs/DEVICE-MATRIX.md` (Samsung SM-X700 tablet, iPad 10th gen on iPadOS
17.4.1). No iPhone and no Windows machine yet.
Open (defaults in brackets):
1. adb distribution: detect + download-with-consent [default] vs build from AOSP.
2. Apple Developer account and Windows signing route [needed in Phase 7].
3. A Windows machine for Phase 3.1 and 7.3 [required before those steps].
4. An iPhone for the iOS matrix [optional; the iPad covers iOS tier 1 and 2].

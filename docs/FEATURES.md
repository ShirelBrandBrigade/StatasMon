# StatasMon feature and data inventory

Every item lists the fields the UI shows. "Source" columns are filled from
`docs/research/*.md`; a field the platform cannot provide is shown as "—" in
the UI, never hidden silently, never faked.

## 0. Devices and sessions
- Connect several devices at once (USB first; Wi-Fi/wireless ADB where the
  platform allows). Each device has its own recording session and its own
  data store; the sidebar switches the active device without stopping others.
- Session: start/pause/resume/stop, elapsed time, marks (user annotations with
  a timestamp), sample count, log entry count. Sessions persist locally and
  can be reopened, compared and exported.
- Platforms: iOS/iPadOS (iPhone, iPad), Android (phone, tablet). Same UI; the
  Device sheet shows platform-specific sections.

## 1. Device information (complete, always visible on the Device sheet)
Common: marketing name, model identifier, manufacturer, platform, OS name,
OS version, OS build number, security patch level (Android), chip/processor
marketing name (A18 Pro, Snapdragon 8 Gen 3), processor identifier
(ChipID / HardwarePlatform, `ro.soc.model`), architecture and ABI list, core
count and core types (performance/efficiency), max frequencies where
available, GPU name where available, RAM total, storage total, serial number,
unique id (UDID / Android serial), device name, uptime and last boot time,
time zone and locale, battery design capacity, developer mode / USB debugging
state, transport (USB-C, Lightning, Wi-Fi, ADB TCP), connection health
(pairing/trust state, tunnel state, last heartbeat), agent/tool versions used.
iOS extras: HardwareModel (D93AP), ProductType, BuildVersion, DeviceClass,
ActivationState, WiFiAddress, BluetoothAddress, RegionInfo, ModelNumber,
firmware/baseband version, jailbroken heuristics = none (do not claim).
Android extras: codename, board, bootloader, kernel version, Play services
version, screen size/density, SELinux mode.

## 2. CPU
Total utilization %, per-core utilization % (each core, labelled by type),
per-core frequency (where available), load average, top consumers by CPU,
history chart (1 s samples, full session).

## 3. Memory
Total, used, free; breakdown (iOS: app/wired/compressed/free; Android:
used/cached/buffers/free, ZRAM/swap), memory pressure state, memory warnings
and jetsam/LMK events with the process that was hit, history chart.

## 4. Thermal
Thermal state (iOS: Nominal/Fair/Serious/Critical; Android: thermal status
0–6 with names), per-sensor temperatures where readable (SoC, battery, PMU,
display, skin), time in current state, state-change events on the timeline.

## 5. Battery
Percent, charging state and source, voltage, current (signed), power
(computed), temperature, cycle count, design vs full-charge capacity and
health %, time to full/empty estimates (computed from current), history chart.

## 6. Storage
Total/used/free; categories (apps, media/photos, system, other, cache) where
available; per-app disk usage (top N); write rate where available.

## 7. Network
Active interface and details (Wi-Fi SSID, RSSI, link speed, band/frequency,
IP addresses; cellular type where readable), download/upload throughput
(bit/s), packets in/out per second, totals per session, errors/dropped/
retransmits where available, per-process/per-app traffic, connection count,
latency probe (optional, opt-in), packet capture start/stop to .pcap where the
platform supports it.

## 8. Processes
Table: name, bundle id/package, pid, CPU %, memory (RSS/PSS), threads,
network rate, energy/impact where available, state (foreground/background);
sort, search, pin; click → per-app view (CPU/mem/net history + its log lines).

## 9. Logs
Unified stream: timestamp (ms), device, process, pid, subsystem/category
(iOS) or tag (Android), level (Debug/Info/Notice/Error/Fault ↔ V/D/I/W/E/F),
message. Filters: app/process (multi), level, free-text and regex, time
range, buffer (Android). Follow-tail with pause, jump to mark, highlight
faults. Crash reports, tombstones and ANRs appear as events with the report
attached. Per-app log = the same stream pre-filtered, plus that app's
metrics.

### Simple mode (2026-09-16, ADR-020)
A **Raw · Simple** switch on both log panels. Simple mode rewrites each
line as a plain sentence for a non-technical reader ("Memory is running
critically low", "The device is running hot and slowing itself down",
"Opened Maps", "FieldKit crashed; a crash report was saved"), colours it
fine / information / worth a look / a problem, folds repeats into one
row with a count, and collapses routine housekeeping into a grey
"Normal background activity" row. A one-line summary and a legend sit
at the top; every row has a Raw toggle showing the original lines. The
rules run on the device only; nothing is sent anywhere. Filters apply
before folding; export always writes raw lines.


## 10. Export
Formats: `.log` (plain text, one line per entry), `.jsonl` (one JSON object
per line, full fields), `.csv` (metrics samples), session bundle `.zip`
(device info + metrics CSV + logs JSONL + marks + crash reports + pcap).
Scopes: current filter, selected rows, time range, whole session, one app.
Export never blocks the UI and reports a path when done.

## 11. Cross-cutting UX
Plain-language headline from rules (thermal ≠ nominal, memory pressure,
battery hot, high retransmits, app crashed); range selector shared by all
charts; marks; keyboard: ⌘/Ctrl+F filter, Space pause tail, ⌘/Ctrl+E export;
offline-first, no network calls by the app itself except an opt-in update
check; every value shows its unit and sampling age.

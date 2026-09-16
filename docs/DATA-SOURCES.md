# Where every number comes from, and how to read the log

Every value StatasMon shows is read from the device through a documented
service; nothing is estimated on the host. This page names the service and
the exact key behind each field, points at the code that reads it, and shows
raw samples captured from the real iPad (10th generation, iPadOS 17.4.1) on
2026-09-10. The Android column comes from the Samsung SM-X700 (Android 16).

## 1. iOS / iPadOS

The app talks to the device over USB through `usbmuxd`, the same daemon
Finder and Xcode use, with the `idevice` crate (ADR-004). Two tiers:

| Tier | Needs | Services |
| --- | --- | --- |
| 1 · always | the device trusted this Mac | `lockdownd`, `diagnostics_relay`, `installation_proxy`, `os_trace_relay`, `crashreportcopymobile`, `pcapd` |
| 2 · Developer Mode on | the developer tunnel (`core_device_proxy` + RSD, `crates/device-ios/src/tunnel.rs`) | DVT `sysmontap`, `networking`, `deviceinfo`, energy gauge |

### Device information (Device page)
Source: `lockdownd` `GetValue` on the root domain and four sub-domains,
read in `crates/device-ios/src/info.rs` and `lockdown.rs`.

| Field | lockdownd key (domain) |
| --- | --- |
| Device name | `DeviceName` |
| Marketing name | `MobileGestalt` marketing name, else `ProductType` mapped |
| Model identifier / number | `ProductType` / `ModelNumber` |
| Serial, UDID, region | `SerialNumber`, `UniqueDeviceID`, `RegionInfo` |
| OS name / version / build | `DeviceClass`, `ProductVersion`, `BuildVersion` |
| Baseband / firmware | `BasebandVersion`, `FirmwareVersion` |
| Chip, board, architecture | `ChipID`, `HardwareModel`, `CPUArchitecture` |
| CPU count, P/E split | DVT `deviceinfo` → `hardwareInformation` (`numberOfCpus`, `hwCPUtype`); the P/E split from sysmontap's `EnabledCPUs` layout |
| RAM | DVT `deviceinfo` `physMemSize` (falls back to sysmontap `physMemSize` × page size) |
| Storage totals | `com.apple.disk_usage`: `TotalDiskCapacity`, `TotalDataCapacity`, `TotalDataAvailable`, `TotalSystemCapacity`, `AmountDataReserved` |
| Per-app storage | `installation_proxy` `Browse` with `ReturnAttributes` `CFBundleIdentifier`, `CFBundleDisplayName`, `StaticDiskUsage`, `DynamicDiskUsage` (`storage.rs`) |
| Time zone, locale | `TimeZone`; `com.apple.international` `Locale`, `Language` |
| Developer Mode | `amfi` `DeveloperModeStatus` |
| Battery design capacity, cycles | `IOPMPowerSource` (below) |

Raw sample, `fixtures/ios/dvt/iPad13,18-17.4.1/hardware_information.plist`:
```
"numberOfCpus" => 6   "numberOfPhysicalCpus" => 6   "hwCPUtype" => 16777228
```
Raw sample, the `info` message recorded live (`fixtures/fake/live-ipad.jsonl`):
```
"identity": {"name": "adbox’s iPad (2)", "marketing_name": "iPad (10th generation)",
 "model_identifier": "iPad13,18", "model_number": "MPQ13RK/A", "serial_number": "CJ2X…",
 "unique_id": "00008101-000E0C94348B001E", "region": "RK/A"},
"software": {"os_name": "iPadOS", "os_version": "17.4.1", "os_build": "21E236", …
```

### Battery (Battery card, Performance)
Source: `diagnostics_relay` `IORegistry` query for the `IOPMPowerSource`
class, every 3 s (`crates/device-ios/src/battery.rs`).

| Metric | IORegistry key | Conversion |
| --- | --- | --- |
| `bat.percent` | `CurrentCapacity` / `MaxCapacity` (or `AppleRawCurrentCapacity`/`AppleRawMaxCapacity`) | percent |
| `bat.charging` | `IsCharging`, `ExternalConnected`, `FullyCharged` | 0/1 |
| `bat.voltage` | `Voltage` | mV → V |
| `bat.current` | `InstantAmperage` | mA → A (negative = discharging) |
| `bat.power` | voltage × current | W |
| `bat.temp`, `temp.battery` | `Temperature` | centi-°C → °C |
| `bat.cycles`, `bat.design_cap`, `bat.health` | `CycleCount`, `DesignCapacity`, `NominalChargeCapacity` | health = nominal / design |

Raw sample (live recording, first `samples` message):
```
bat.percent 100, bat.charging 0, bat.voltage 4.272, bat.current 0, bat.temp 28.5, bat.cycles 76
```

### CPU, memory, processes (CPU/Memory cards, Performance, Apps)
Source: DVT `com.apple.instruments.server.services.sysmontap`, the same
feed Xcode Instruments uses, once a second (`crates/device-ios/src/dvt/sysmontap.rs`).
The first message carries `SystemAttributes` / `ProcessesAttributes` (the
column names), every later one carries arrays in that order.

| Metric | sysmontap key | Note |
| --- | --- | --- |
| `cpu.total` | `SystemCPUUsage.CPU_TotalLoad` ÷ `CPUCount` | the load is summed over cores on the device (verified: 74.4 with 6 cores at ~12 %) |
| `cpu.user`, `cpu.system` | `CPU_UserLoad`, `CPU_SystemLoad` ÷ cores | |
| per-core `usage_pct` | `PerCPUUsage[i].CPU_TotalLoad` | 0–100 each; negative = unavailable |
| `mem.total` | `physMemSize` × `vmPageSize` | pages, not bytes, on this device |
| `mem.used`, `mem.app`, `mem.wired`, `mem.compressed`, `mem.free` | `PhysicalMemoryActive`+`Inactive`, `PhysicalMemoryWired`, `PhysicalMemoryCompressed`, `PhysicalMemoryFree` | pages × page size |
| `mem.swap_*`, `mem.pageins` | `vmSwapIns/Outs`, `vmPageIns/Outs` | |
| per-process `cpu_pct` | `cpuUsage` | already percent of one core (a busy process can exceed 100) |
| per-process `mem_bytes`, `threads` | `physFootprint` (else `memResidentSize`), `threadCount` | bytes |
| process `app_id` | `bundleID` | null for daemons |
| `disk.read_bps`, `disk.write_bps` | `diskBytesRead`, `diskBytesWritten` deltas | |

The system-wide `netBytesIn/Out` from sysmontap are used only while the
networking service is down; they include loopback and the tunnel itself.

Raw capture of one tick: `fixtures/ios/dvt/iPad13,18-17.4.1/sysmontap_messages.plist`
(binary plist; `sysmon_attributes.plist` lists the 60 process columns the
device advertises, among them `cpuUsage`, `physFootprint`, `threadCount`,
`bundleID`, `pid`, `name`).

### Network (Network card and page)
Source: DVT `com.apple.instruments.server.services.networking`
(`crates/device-ios/src/dvt/network.rs`). It streams three event kinds:
`InterfaceDetection` (index → name), `ConnectionDetection` (serial, pid,
interface, addresses) and `ConnectionUpdate` (rx/tx bytes and packets,
retransmits, RTT). Facts established on the iPad:

- Only `en0`/`en2`-style interfaces count as device traffic; `lo0`, `utun*`
  (VPN and the developer tunnel), `anpi*`, `awdl*`, `ipsec*` are excluded.
- A connection's first update carries lifetime totals; every later update
  carries the interval's bytes (`networking_events.plist`: serial 296
  reports 3 012 273 rx bytes, then 260, 63, 71, 71…).
- `net.rx_bps`, `net.tx_bps`, `net.rx_pps`, `net.tx_pps`, `net.rx_total`,
  `net.tx_total`, `net.connections`, `net.retransmits`, `net.latency_ms`
  (average of reported RTTs) and per-process `net_bps` all derive from this.
- Wi-Fi details (RSSI, link speed, SSID, IP) are not exposed by any
  developer service, so they show "—" with that reason.

### Thermal state and events
iOS gives the host no thermal query. `thermal.state` comes from what
`thermalmonitord` writes to the unified log ("thermal state: Nominal|Fair|
Serious|Critical"), parsed in `crates/device-ios/src/thermal.rs`, and the
sensors are the battery temperature above. Memory warnings, jetsam kills
and crashes are recognised from log lines the same way (rules listed at
the top of `thermal.rs`); crash reports themselves are pulled with
`crashreportcopymobile` (`crash.rs`, `.ips` files under
`/private/var/mobile/Library/Logs/CrashReporter`).

### Logs
Source: `os_trace_relay` (`crates/device-ios/src/ostrace.rs`), the unified
log stream. Each record gives the pid, the sender path (the process name is
its basename), the emitting image, the subsystem/category label, the level
and the message. Packet capture uses `pcapd` (`pcap.rs`).

## 2. Android

Everything goes through `adb shell` (`crates/device-android/src/samplers`,
`shell.rs`), with one persistent shell per device. The exact commands:

| Field | Command |
| --- | --- |
| Device information | `getprop` (`ro.product.*`, `ro.build.version.*`, `ro.build.version.sdk`, `ro.hardware`, `ro.board.platform`), `cat /proc/version`, `settings get global development_settings_enabled / adb_enabled / adb_wifi_enabled` |
| CPU total, per core | `cat /proc/stat` deltas; `cat /sys/devices/system/cpu/online`, `possible`, `cpu*/cpufreq/scaling_cur_freq` |
| Load average, uptime | `cat /proc/loadavg`, `cat /proc/uptime` |
| Memory | `cat /proc/meminfo` (`MemTotal`, `MemAvailable`, `Cached`, `SwapTotal/Free`), `cat /proc/pressure/memory` when readable, `dumpsys -t 5 meminfo` (opt-in, heavy) |
| Battery | `dumpsys -t 5 battery` (`level`, `voltage`, `temperature`, `status`, `health`, `current now`) |
| Thermal | `dumpsys -t 5 thermalservice` (status and per-sensor temperatures) |
| Storage | `df -k /data`, `dumpsys -t 5 diskstats` |
| Network | `cat /proc/net/dev` deltas (bytes and packets per interface), `dumpsys -t 5 netstats detail` |
| Processes | `/proc/<pid>/stat` per pid, `dumpsys -t 5 activity activities` for the foreground app |
| Logs, ANRs, crashes | `logcat -v threadtime -v epoch -v uid -v usec -b main,system,crash,events -T 1`; `dumpsys -t 5 dropbox` |

Where a command is refused by the shell user (for example `/proc/stat` on
some Samsung builds) the capability is withdrawn with the reason you see in
the tooltip, never guessed.

## 3. Reading the log panel

Each row is one record from the device: **time** (device clock, to the
millisecond where the platform gives it; iPadOS 17 gives whole seconds),
**process** (the executable name, shortened; hover for the full sender and
subsystem), **level** pill, and the **message** verbatim. The tile on the
left marks the app under test.

Levels are mapped to one scale (`packages/protocol` `LogLevel`):

| Pill | iOS unified log | Android logcat | Meaning |
| --- | --- | --- | --- |
| Debug | Debug | V, D | developer chatter, hidden by default |
| Info | Info | I | informational, hidden on iOS by default (see below) |
| Notice | Default | – | what apps and daemons log normally |
| Warning | – | W | something worth a look |
| Error | Error | E | a failed operation |
| Fault | Fault | F, A | a serious failure; rows are outlined red |

An idle iPad writes about 4 400 lines a second with every process traced,
71 % of them Debug. To keep the panel readable the iOS stream is admitted
as follows (`ostrace.rs`, ADR-016/017): Notice and above; at most 2 lines
per second per process (Error 5 per second, Fault never limited); 200
lines per second overall. Every 10 s a synthetic line from process
**StatasMon** says exactly what was hidden, for example the one recorded:
```
StatasMon hid 56078 lines below Notice and 7360 over the 200/s limit in the last 10 s
(set STATASMON_IOS_LOG_MIN_LEVEL=debug to keep everything)
```
`STATASMON_IOS_LOG_MIN_LEVEL=info|debug` and `STATASMON_IOS_LOG_PER_PROCESS=0`
lift the limits.

What the common iPadOS lines mean (all from the 2026-09-10 recording):

- `runningboardd · [osservice<com.apple.mediaplaybackd>:432] is not RunningBoard jetsam managed.` — RunningBoard (the process manager) noting that a system service is exempt from memory-pressure kills. Routine; it is *not* a kill, and the app does not count it as one.
- `lockdownd · handle_get_value: <private>` — lockdownd answering a property read. StatasMon's own tier-1 polling causes many of these.
- `mobile_diagnostics_relay · System Keychain Always Supported set via feature flag to disabled` — the diagnostics service (which serves the battery query) starting up.
- `trustd · [0xc8eab7200] invalidated after getting a no-senders notification - client is gone` — an XPC connection closed normally.
- `kernel · if_updown: utun6 is not attached` — a tunnel interface (the developer tunnel uses `utun`) going up or down.
- `wifid · LQM-WiFi: WiFi Latency: TxLat-hist …` (Error level) — Wi-Fi link-quality telemetry that wifid logs at Error level about eight times a second; it is noise, capped to 5 lines a second.
- `duetexpertd · ATXMemoryPressureMonitor: received memory pressure warning of type: critical` — iPadOS told this daemon memory pressure was critical. This is a real signal and becomes the **Memory warning** pill and a chart marker.
- `<private>` inside a message is the device redacting a value; StatasMon cannot see it either.

Events shown as chart markers and in the Logs page strip are derived from
these lines only where the text is unambiguous: a `thermalmonitord` state
change, a `memorystatus` kill or `jetsam` policy line (kill verbs required),
a memory-pressure warning, a `ReportCrash`/`osanalyticsd` report. Identical
events within 30 s are one incident.

## 4. How to verify this yourself
- Live capture from a connected iPad, exactly what the app receives:
  `cargo run -p statasmon-device-ios --example record_live -- out.jsonl 60`
- Raw DVT fixtures from the device: `cargo run -p statasmon-device-ios --example capture_dvt_fixture`
  (writes the plists under `fixtures/ios/dvt/<model>-<version>/`).
- The provider tests replay those fixtures: `cargo test -p statasmon-device-ios`.
- Research notes with Apple and Android references: `docs/research/ios.md`, `docs/research/android.md`.

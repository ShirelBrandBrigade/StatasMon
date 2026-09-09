# Research: iOS/iPadOS telemetry from a desktop host (September 2026)

Compiled by a research agent from current sources. Legend: L = lockdownd
(no tunnel), D = diagnostics_relay (no tunnel), DVT = Instruments over the
RSD tunnel (iOS 17+). ✔ yes · ◐ partial · ✘ no.

## 1. Library overview

| | libimobiledevice (C) | pymobiledevice3 (Python) | go-ios (Go) | `idevice` (Rust, jkcoxson) |
|---|---|---|---|---|
| License | LGPL-2.1 lib / GPL tools | **GPL-3.0** | MIT | MIT |
| 2026 activity | 1.4.0 (Oct 2025); status page stops at iOS 13.5 | very active, v11.10.x Sept 2026 | active, v1.3.2 Aug 2026; instruments network/battery/sysmontap since v1.1 | active, 0.1.65 on crates.io Jul 2026, ~25k LoC, pre-0.2 (breaking changes) |
| macOS / Windows | both | both; Windows needs iTunes | both; Windows kernel tunnel needs wintun.dll | both (usbmuxd unix socket or TCP 127.0.0.1:27015 on Windows); pairing tool says "install iTunes" on Windows |
| iOS 17+ tunnel / DVT | **no** RemoteXPC/instruments | yes, in-process userspace tunnel, no root, iOS 17.4+ | yes: `sudo ios tunnel start` (kernel TUN) or userspace agent | yes: `core_device_proxy` + `rsd` + `tunnel_tcp_stack` (userspace TCP), or bridge to pymobiledevice3 tunneld |

Other Rust: `rusty_libimobiledevice` (FFI), `ios-rs` (immature),
`rust-ios-device` / `rust-ios-device-tunnel` (userspace CoreDevice tunnel
wheel, Sept 2026). Node: appium-ios-device / appium-ios-remotexpc (Apache-2,
remotexpc macOS/Linux only, root for TUN).

Sources: github.com/libimobiledevice, github.com/doronz88/pymobiledevice3,
github.com/danielpaulus/go-ios, github.com/jkcoxson/idevice, docs.rs/idevice.

## 2. Capability matrix

| Telemetry | Service / key | libimobiledevice | pymobiledevice3 | go-ios | idevice (Rust) |
|---|---|---|---|---|---|
| CPU total | DVT `sysmontap` → `SystemCPUUsage.CPU_TotalLoad` | ✘ | ✔ | ✔ | ✔ `dvt/sysmontap.rs` (raw plist) |
| CPU per core | sysmontap system attribute **`PerCPUUsage`** (array of CPU_TotalLoad/User/System/Nice per core) + `CPUCount`, `EnabledCPUs` | ✘ | ✔ (request the attribute) | ◐ (struct hides it; present in payload) | ◐ (pass attribute; parse raw) |
| Memory breakdown / pressure | sysmontap sysAttrs `PhysicalMemoryFree/Active/Inactive/Wired`, compressor and swap counters | ✘ | ✔ | ✔ | ◐ raw |
| Thermal state | **No host-side query API.** Options: tail os_trace/syslog for `thermalmonitord` state transitions; reverse Xcode's Thermal State instrument channel (undocumented); `ConditionInducer` only *sets* simulated conditions | ◐ syslog | ◐ os_trace | ◐ syslog | ◐ os_trace |
| Temperature sensors | D `ioregistry` class `IOPMPowerSource` / `AppleSmartBattery` → `Temperature` (e.g. 3980 = 39.8 °C). **Battery only; no SoC/skin sensors** | ✔ | ✔ | ✔ | ✔ `diagnostics_relay` |
| Battery % / charging | L domain `com.apple.mobile.battery` (`BatteryCurrentCapacity`, `BatteryIsCharging`, `ExternalConnected`) | ✔ | ✔ | ✔ | ✔ |
| Voltage, current, cycles, capacities | D IORegistry `AppleSmartBattery`: `CycleCount`, `DesignCapacity`, `AppleRawMaxCapacity`, `Voltage`, `InstantAmperage`, `Temperature`; also `GasGauge` | ✔ | ✔ | ✔ | ✔ |
| Storage total/used/free | L domain `com.apple.disk_usage` (`TotalDiskCapacity`, `TotalDataCapacity`, `TotalDataAvailable`, `AmountDataReserved`…) | ✔ | ✔ | ✔ | ✔ |
| Storage per app | installation_proxy Browse/Lookup with `StaticDiskUsage`, `DynamicDiskUsage` | ✔ | ✔ | ✔ | ✔ |
| Network bytes/packets per s | sysmontap sysAttrs `NetBytesIn/Out`, `NetPacketsIn/Out` | ✘ | ✔ | ✔ | ◐ raw |
| Per-interface / per-process network | DVT `com.apple.instruments.server.services.networking`: interface-detection, connection-detection (pid, addresses, interface), connection-update (rx/tx bytes, packets, RTT) | ✘ | ✔ | ✔ | ✔ `network_monitor.rs` |
| Raw packets | `com.apple.pcapd` (no tunnel) | ✔ | ✔ | ✔ | ✔ `pcapd` |
| Processes CPU/mem/threads | sysmontap procAttrs (`cpuUsage`, `physFootprint`, `memResidentSize`, `threadCount`, `ctxSwitch`, `diskBytesRead`…); `deviceinfo.runningProcesses` static list | ✘ | ✔ | ✔ | ✔ |
| Logs (level/pid/subsystem) | `com.apple.os_trace_relay` (structured, no tunnel) vs `com.apple.syslog_relay` (raw text) | ◐ syslog | ✔ | ◐ syslog | ✔ `os_trace_relay` |
| Crash reports | `com.apple.crashreportmover` then AFC over `com.apple.crashreportcopymobile` | ✔ | ✔ | ✔ | ✔ |
| Device info | L `GetValue`: `ProductType`, `ProductVersion`, `BuildVersion`, `HardwarePlatform`, `ChipID`, `CPUArchitecture`, `HardwareModel`, `SerialNumber`, `UniqueDeviceID`, `DeviceName`, `ModelNumber`, `RegionInfo`, `WiFiAddress`, `BluetoothAddress`, `DeviceClass`, `ActivationState`, `TimeZone`; RAM via D mobilegestalt or DVT `deviceinfo.hardwareInformation` | ✔ | ✔ | ✔ | ✔ |
| Uptime | DVT `deviceinfo.machTimeInfo` / sysmontap `EndMachAbsTime` (derive) | ✘ | ◐ | ◐ | ◐ |
| Developer Mode state | L domain `com.apple.security.mac.amfi` key `DeveloperModeStatus`; enable via `com.apple.amfi.lockdown` | ✔ | ✔ | ✔ | ✔ `amfi` |
| Energy per process | DVT `com.apple.xcode.debug-gauge-data-providers.Energy`; broken on some iOS 18 builds | ✘ | ✔ | ✘ | ✔ `energy_monitor.rs` |

## 3. iOS 17+ tunnel (RemoteXPC / CoreDevice)
- All DVT rows need an RSD tunnel; lockdown, diagnostics, os_trace, pcapd
  and crash rows do not.
- iOS 17.0–17.3.1: tunnel needs a root daemon (`tunneld`) on every OS
  (Windows also wintun). Treat as legacy tier.
- iOS 17.4+: `CoreDeviceProxy` lockdown service over USB, **no root**, via
  an in-process userspace TCP stack (pymobiledevice3, idevice
  `tunnel_tcp_stack`, rust-ios-device).
- iOS 18.2+ dropped QUIC; tunnels use TCP + TLS-PSK. go-ios ≥1.2 and
  pymobiledevice3 handle it; idevice tracks it.
- iOS 26 (26.6.1 current): pymobiledevice3 and idevice track it; go-ios has
  an open iOS 26 tunnel issue.
- Wi-Fi: iOS 17.4+ can enable `com.apple.mobile.wireless_lockdown /
  EnableWifiConnections` and discover via `_apple-mobdev2._tcp`; earlier
  17.x uses RemotePairing after a USB bootstrap. Device-initiated Wi-Fi
  pairing needs iOS 27+.

## 4. Windows specifics
- Apple's `AppleMobileDeviceService.exe` (iTunes / "Apple Devices" from the
  Microsoft Store) owns the USB driver and listens on TCP 127.0.0.1:27015;
  every library connects there. No practical open-source replacement
  (`netmuxd` can evict iTunes but is disruptive).
- Pairing/trust is the standard lockdown "Trust This Computer" flow; pair
  records are stored by Apple's service.
- Kernel tunnels need wintun.dll + admin; userspace tunnels need neither.
- Wi-Fi devices show as `ConnectionType: Network` only when Wi-Fi sync is on.

## 5. Identifier → marketing name / chip mapping
- ProductType → name: github.com/fieldnotescommunities/ios-device-identifiers
  (JSON), adamawolf gist, ElfSundae/iOS-Model-List.
- ProductType + SoC + arch: **AppleDB** (github.com/littlebyteorg/appledb,
  static JSON at api.appledb.dev) — best single source (iPhone17,1 → A18 Pro).
- ChipID/HardwarePlatform → chip: theapplewiki.com/wiki/CHIP (T8130 = A17
  Pro, T8140 = A18 Pro, T8140a = A18).

## 6. Gotchas
1. Per-core CPU, memory pressure and packets/s all come from sysmontap
   system attributes; query `deviceinfo.sysmonSystemAttributes` at runtime,
   the list varies by iOS version.
2. No host-side "current thermal state" API: tail os_trace for
   `thermalmonitord` transitions and keep last-known state, or reverse the
   Instruments Thermal State channel.
3. Only battery temperature is readable.
4. pymobiledevice3 is GPL-3: cannot be embedded in a proprietary app.
5. Energy gauge and some DVT channels break across iOS majors.
6. libimobiledevice cannot reach instruments on iOS 17+.

## 7. Recommended approach
Rust `idevice` in the Tauri core (primary), go-ios as an optional fallback
CLI. idevice is MIT, pure Rust, single static binary on both OSes,
implements every service in the matrix, and its `tunnel_tcp_stack` gives a
no-root userspace tunnel. Use lockdown + diagnostics polling for battery,
storage and device info (works without Developer Mode); open DVT sysmontap
and networking only when Developer Mode is on. Verify against iOS 26 in CI;
the crate is pre-0.2. Keep pymobiledevice3 for prototyping/verification only.

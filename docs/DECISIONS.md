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

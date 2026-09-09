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

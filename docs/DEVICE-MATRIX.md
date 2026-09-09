# Device matrix

Real devices available for verification. Update when a device or OS
version changes. Every provider step in `docs/PLAN.md` must be verified on
the devices listed for its platform.

| Device | Model id | Platform / OS | Transport | Notes |
|---|---|---|---|---|
| Samsung Galaxy Tab S8 (Wi-Fi) | SM-X700 | Android (version: record on first connect via `getprop ro.build.version.release`) | USB, wireless ADB | Samsung One UI: expect vendor SELinux labels on thermal zones; verify `dumpsys thermalservice`, `power_supply` cycle_count, `dumpsys diskstats` |
| iPad (10th generation) | iPad13,18 / iPad13,19 (A14) | iPadOS 17.4.1 | USB-C | Tier 1 (lockdown, diagnostics, os_trace, pcapd, crashes) without Developer Mode; Tier 2 via CoreDeviceProxy userspace tunnel (17.4+, no root, QUIC era: pre-18.2). Confirm exact ProductType on first connect |

Missing: an iPhone (optional), a Windows machine (required before Phase
3.1 and 7.3), an Android phone (optional; the tablet covers the provider).

Host: macOS (Darwin 25.5), Apple Silicon assumed; adb 37.0.1 at
`~/Downloads/platform-tools`.

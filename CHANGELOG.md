# Changelog

All notable changes to StatasMon are recorded here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/) and the project
uses [Semantic Versioning](https://semver.org/). Unreleased changes are
listed first; a release moves them under a dated version heading
(`docs/RELEASE.md`).

## [Unreleased]

### Added
- Every push to main publishes a versioned GitHub release (0.1.<commit count>) with installers for macOS (arm64, x86_64), Windows and Linux.
- adb (Android platform-tools) ships inside every build on every OS; nothing to download for Android (ADR-021).
- Linux x86_64 installers (.AppImage and .deb); the .deb pulls in usbmuxd for iPhone/iPad.
- Per-OS instructions the moment Apple's USB service is missing (Apple Devices app on Windows, usbmuxd on Linux).
- Simple mode for the log: plain-language stories with a Raw toggle (ADR-020).
- Explanation of every data source and of the log in docs/DATA-SOURCES.md; docs/GETTING-STARTED.md to send with the installer.


### Added
- **Several devices at once.** iPhones, iPads and Android phones and
  tablets connect over USB (Android also over wireless ADB) and each gets
  its own live session; the sidebar switches between them without
  stopping the others.
- **Android (12 and later)** over `adb`: CPU total and per core with
  frequencies, memory, thermal status and sensor temperatures, battery
  (level, charging, voltage, current, temperature, health, cycle count
  where the vendor exposes it), storage by category and per app, network
  throughput and packets with Wi-Fi details, processes with CPU and memory,
  the full `logcat` stream, crashes and ANRs, complete device information
  (model, SoC, build, security patch, ABIs, kernel, uptime). `adb` is found
  automatically (SDK, PATH, Downloads); the path can also be set in
  Settings. Wireless pairing and connecting, and bugreport capture, from
  the "Add a device" dialog.
- **iOS / iPadOS (17.4 and later)** in two tiers. Tier 1 needs only
  "Trust This Computer": battery (level, charging, voltage, current,
  temperature, cycle count, capacities), storage total and per app, the
  complete device information sheet (marketing name, chip, model
  identifiers, serial, UDID, region, MAC addresses), the structured
  system log, crash reports and packet capture to `.pcap`. Tier 2 turns on
  automatically when Developer Mode is enabled: CPU total and per core,
  memory breakdown, per-process CPU and memory, network throughput and
  packets, per-app network. No jailbreak or extra tools.
- **Sessions.** Recording starts when a device attaches; pause, resume
  and stop; marks with labels drawn on every chart; sessions are kept
  locally (4 hours per session, 30 days by default) and can be reopened
  from the Sessions screen with their device info, marks and events.
- **Export** to `.log` (plain text), `.jsonl` (every field), `.csv`
  (metrics, wide or long) or a `.zip` bundle with device info, metrics,
  logs, events, marks, crash reports and captures; progress is shown and
  the destination is chosen by you.
- **Screens.** Overview (headline, rings, sparklines), Performance (CPU,
  memory, thermal, battery with real axes and a shared range selector),
  Network, Apps & processes with an app detail view, Logs (virtualized,
  filter with ⌘/Ctrl+F, pause the tail with Space, full-text search across
  the session), Device (the information sheet), Sessions, Settings.
- **Settings** stored in `settings.json`: sampling cadences, retention,
  `adb` location, and the opt-in "Check for updates on launch".
- **Installers and updates.** macOS `.dmg` (Apple Silicon and Intel,
  macOS 12+), Windows per-user `.exe` (NSIS) and `.msi`. The in-app update
  check runs only when you opt in; it announces a new version and installs
  it only when you ask; only artifacts signed with the project key are
  accepted.
- A deterministic **fake device source** (`pnpm dev:mock`) with scripted
  scenarios, used to build and test every feature before real hardware.

### Known limitations (see `docs/KNOWN-ISSUES.md`)
- The macOS and Windows installers are **not yet signed or notarized**:
  macOS shows "cannot be opened because the developer cannot be verified"
  (use Open anyway in System Settings → Privacy & Security) and Windows
  shows a SmartScreen warning. Signing lands as soon as the certificates
  exist; the pipeline is ready for them.
- Storage sizes use binary units (1 GB = 1024³ bytes), so a "256 GB"
  device shows about 238.4 GB.
- iOS thermal state comes from the device's own thermal log messages, so
  it reads "not reported yet" until the device reports a change; only the
  battery temperature is readable on iOS.
- Android battery current and power are withheld on Samsung devices until
  the unit and sign reported by One UI are verified.
- Per-app memory (PSS) snapshots and detailed crash/ANR reports from
  Android's dropbox are not collected yet; crash events carry the log line.
- Android packet capture is not available without root.
- Wireless ADB pair/connect and bugreport, and the iOS providers, are
  tested against recorded fixtures; live verification on the iPad and a
  wireless-ADB device is pending.
- Sampling, retention and `adb` path changes in Settings take effect at
  the next launch.
- Each Android sample costs about 270 ms through the `adb` subprocess;
  fine at 1 Hz, and a faster transport is planned.
- Live cards freeze while the window is hidden and catch up in one tick
  when it is visible again (recording continues).

[Unreleased]: https://github.com/ShirelBrandBrigade/StatasMon/compare/main...HEAD

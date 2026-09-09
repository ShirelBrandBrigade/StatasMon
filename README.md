# StatasMon

A macOS and Windows desktop app that connects to iPhones, iPads and Android
devices at the same time, records everything they report every second, and
shows it in one dark dashboard: CPU per core, memory, thermal state, battery,
storage, network throughput and packets, processes, logs, crashes, and the
full device information sheet. Sessions are stored locally and export to
`.log`, `.jsonl`, `.csv` or a `.zip` bundle.

Values a device cannot provide are shown as a dash with the reason. Nothing
is fabricated.

## Platforms

| Device | What works | Needs |
|---|---|---|
| Android 12+ | everything except packet capture and battery sysfs fields locked by the vendor | USB debugging on, `adb` (auto-detected or downloaded on first run) |
| iOS / iPadOS 17.4+ | battery, storage, device info, logs, crashes, packet capture without any setup; CPU, memory, processes and per-app network with Developer Mode on | "Trust This Computer"; on Windows, iTunes or the Apple Devices app for the USB driver |

## Develop

```bash
pnpm install
pnpm dev:mock        # the app on three deterministic fake devices
pnpm dev             # real devices over adb / usbmuxd
pnpm check           # Biome + TypeScript + Vitest
cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace
pnpm --filter @statasmon/desktop e2e   # Playwright on the browser replay
```

Rust stable (see `rust-toolchain.toml`), Node 22+, pnpm 12. The plan, the
design system, the decisions and the research live in `docs/`; the working
rules for people and agents are in `CLAUDE.md`.

## Layout

- `apps/desktop` — the React UI (Vite, uPlot, react-virtuoso)
- `packages/ui` — design tokens and primitives; `packages/protocol` — generated types
- `crates/protocol` — the one data contract; `crates/core` — engine, ring buffers, rules
- `crates/device-*` — fake, Android (adb), iOS (idevice) providers
- `crates/store`, `crates/export` — SQLite sessions and exporters
- `src-tauri` — the Tauri 2 shell and commands

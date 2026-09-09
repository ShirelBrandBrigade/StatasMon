# Research: desktop app stack (September 2026)

Compiled by a research agent from current sources; versions are as found in
September 2026. Verify a version before pinning it in a manifest.

## 1. Desktop shell: Tauri 2.x vs Electron vs Wails

| Criterion | Tauri 2.11.x | Electron 44 (Chromium 152, Node 24) | Wails v3 beta |
|---|---|---|---|
| Bundle | ~9 MiB vs ~244 MiB for equivalent apps (gethopp.app/blog/tauri-vs-electron) | 85–250 MB | small |
| Memory | Big win on macOS; on Windows WebView2 is Chromium so parity or worse (tauri issue #5889) | one renderer process per window | same webviews as Tauri |
| Native/USB | Rust core: `nusb`/`rusb`, `quinn` (QUIC), tokio | Node native addons, N-API rebuilds per ABI | Go directly |
| Sidecar | First-class: `bundle.externalBin` with `-$TARGET_TRIPLE` suffix; `shell().sidecar()` streams stdout/stderr events (v2.tauri.app/develop/sidecar) | `child_process.spawn` / `utilityProcess` | `os/exec` |
| High-frequency IPC | Events are "not designed for low latency or high throughput"; use `Channel<T>` (ordered, fast) (v2.tauri.app/develop/calling-frontend). JSON payloads; binary framing via community `tauri-wire` is ~30× faster | `ipcMain`/`ipcRenderer`, `MessagePort` | bound methods + events |
| WebView | WKWebView (WebKit tied to macOS version) + WebView2 (evergreen Chromium) (v2.tauri.app/reference/webview-versions) | same Chromium everywhere | same as Tauri |
| Auto-update | `tauri-plugin-updater`: minisign signatures, static JSON or endpoint; Windows exits to install | Squirrel / update.electronjs.org | none built in |
| Signing | macOS: Developer ID + notarization; Windows: OV cert (SmartScreen until reputation) or EV, Azure Key Vault / Azure Artifact Signing via `signCommand` (v2.tauri.app/distribute/sign/*) | same certs via electron-builder/Forge | same certs |
| Maturity | stable 2.x since Oct 2024 | very mature | beta with many open gates (wails issue #5844) |

Frosted glass: `backdrop-filter` works in WKWebView and WebView2 (caniuse).
Do NOT set the Tauri window `transparent: true` on macOS: forces full-window
alpha compositing (~8× GPU power, tauri #15471) and breaks backdrop blur
(#12804, #13801). Keep blur to a handful of large cards over an opaque dark
ground. Canvas 2D is GPU-composited in both engines.

Known Tauri 2 pain points: WebKit vs Chromium rendering quirks (test both);
slow Rust cold builds; WebView2 first-launch latency; no WKWebView WebDriver
so `tauri-driver` e2e is Windows/Linux only (v2.tauri.app/develop/tests/webdriver);
JSON-only IPC by default.

Verdict: Tauri 2. Rust core is the natural home for USB, QUIC, SQLite;
sidecars are first-class; installers ~30× smaller.

## 2. Frontend framework and state
Solid/Svelte 5 beat React on raw DOM throughput, but Solid 2.0 is
mid-migration with breaking changes and Svelte's virtual-list/headless
ecosystem is thinner. Hot paths (chart canvas, log virtualizer) bypass the
framework anyway.

Recommendation: React 19.2.x + TypeScript 5.x + React Compiler 1.0
(react.dev/blog/2025/10/07/react-compiler-1). Largest corpus for AI agents,
deepest library pool. State: Zustand 5 for app/session state; the telemetry
ring buffer lives OUTSIDE React (typed arrays fed by the Tauri Channel,
subscribed transiently so uPlot `setData` runs without re-renders).

## 3. Charting

| Library | min+gz | Renderer | 1 Hz × 3,600 pts × many series | Notes |
|---|---|---|---|---|
| uPlot 1.6.x | ~22 KB | Canvas | excellent (author bench: 34 ms render vs Chart.js 38 / ECharts 55) | no animations; imperative |
| Chart.js 4.5 | 68 KB | Canvas | good with `animation:false` | needs streaming plugin |
| ECharts 6.1 | 368 KB (tree-shakable ~100 KB) | Canvas/SVG | good, heavier | rich |
| Recharts 3.10 | 148 KB | SVG/React | poor at 3,600 nodes/series | declarative |
| visx 4 | 50 KB | SVG | poor for streaming | low level |
| Observable Plot | 128 KB | SVG | rebuilds DOM per update | exploratory |

Lines/areas: uPlot wrapped in one React component. Rings and bar meters:
plain SVG + CSS (`stroke-dasharray`, transitions), no library.

## 4. Virtualized log viewer
react-virtuoso 4.18.x: automatic height measurement, `followOutput`,
`atBottomStateChange` (virtuoso.dev). TanStack Virtual 3.14 is headless
(more code, total control). react-window 2.3 lacks follow-tail. Keep lines
in a bounded ring buffer in Rust, filter in Rust, ship batched pages
(100–500 lines per channel message). Pick Virtuoso.

## 5. Session storage
`rusqlite` 0.40.x in the Rust core (bundled SQLite, WAL) rather than
`tauri-plugin-sql` (raw SQL from the webview, no JS transaction API). One
writer batches 1 Hz samples plus log lines per transaction. DuckDB is
OLAP-oriented and adds ~20 MB; not for live appends.
Schema sketch: `sessions(id, device_id, started_at, ended_at, meta_json)`;
`samples(session_id, ts_ms, metric_id, value REAL)` indexed
`(session_id, metric_id, ts_ms)`; `metrics` lookup; `logs(session_id, seq,
ts_ms, level, tag, pid, message)` + FTS5 on message; process snapshots as
JSON per tick. Retention: per-session cap, delete sessions older than N
days, periodic `wal_checkpoint`. Export from Rust with `csv`, `serde_json`,
`zip`; write through dialog/fs plugins.

## 6. Tooling for humans + AI agents
pnpm workspace (`apps/desktop`, `packages/ui`, `packages/protocol` with TS
types generated from Rust via `ts-rs`/`specta`) + Cargo workspace
(`crates/*`, `src-tauri`). Vite 8, Vitest 5 (browser mode), Playwright 1.63
against the Vite dev server with a mock device; WebdriverIO for occasional
real-shell smoke tests. `cargo test` + `cargo clippy`. Biome 2.5 for
lint/format (add ESLint only for `react-hooks`/type-aware rules).
Conventional Commits + commitlint, changesets. CI: Tauri GitHub template,
matrix `macos-latest` (aarch64 + x86_64) and `windows-latest`,
`tauri-apps/tauri-action@v1`, `dtolnay/rust-toolchain`, `swatinem/rust-cache`,
signing secrets via env (v2.tauri.app/distribute/pipelines/github).
Fake device: a `DeviceSource` trait with `UsbSource` and `FakeSource`
(seeded, replayable), same protocol as real sources; `pnpm dev:mock`.

## 7. Design tokens
CSS custom properties on `:root` using `oklch()` (Safari 15.4+, Chromium
111+), exposed through Tailwind v4 `@theme` so utilities and charts read the
same values. Fonts via `@fontsource-variable/manrope` and
`@fontsource-variable/jetbrains-mono` (woff2 in node_modules, bundled by
Vite, no runtime Google Fonts). Lint for hard-coded hex.

## Recommended stack
- Shell: Tauri 2.11.x; plugins shell, updater, dialog, fs, log. Rust backend
  (`nusb`, `quinn`, `tokio`, `rusqlite`, `zip`, `csv`). Sidecar only if a
  driver library forces it.
- Frontend: React 19.2, TypeScript 5, React Compiler 1.0, Zustand 5,
  Tailwind v4 + oklch CSS tokens, Radix primitives.
- Charts: uPlot 1.6; hand-rolled SVG gauges.
- Logs: react-virtuoso 4.18.
- Tooling: pnpm + Cargo workspaces, Vite 8, Vitest 5, Playwright 1.63,
  Biome 2.5, Conventional Commits, GitHub Actions with tauri-action.
- Distribution: Developer ID + notarization (macOS); Azure Artifact Signing
  or EV cert (Windows); tauri-plugin-updater with static JSON on releases.

# StatasMon — working rules for Claude and sub-agents

StatasMon is a macOS + Windows desktop app that connects to iPhones, iPads
and Android devices, records everything they report (CPU, memory, thermal,
battery, storage, network, processes, logs, device information) and shows it
in one dark "glass" dashboard. Read these files before doing anything:

1. `docs/PLAN.md` — the development plan: architecture, phases, current step.
2. `docs/DESIGN-SYSTEM.md` — tokens and components. UI work follows it exactly.
3. `docs/FEATURES.md` — every feature and field. Nothing is silently dropped.
4. `docs/research/*.md` — what each platform can and cannot provide, with sources.
5. `docs/DECISIONS.md` — decision log (ADRs). Add one when you change a decision.

## Ground rules (apply to every agent, every task)
- **Work from the plan.** Each task names the phase and step in `docs/PLAN.md`
  it implements. If a step is unclear or wrong, fix the plan first (small
  edit + ADR), then implement. Do not invent scope.
- **Truth over polish.** A metric the platform cannot provide is shown as
  "—" with a tooltip saying why. Never fabricate, smooth over, or hide.
- **Never block the UI.** Device I/O, parsing, database writes and exports
  run in the core (Rust) or in workers; the UI thread only renders.
- **One data contract.** All device data crosses the process boundary as the
  typed events defined in `packages/protocol` (JSON schema + TypeScript +
  Rust types generated from one source). Changing a field means changing
  the schema, the generators, the fake device, the tests and the docs.
- **Fake device first.** Every feature is built against the `fake` device
  source (deterministic, scriptable) before real hardware, and every
  provider ships a fixture recorded from a real device.
- **Small, verified steps.** One PR-sized change per task: `pnpm check`
  (Biome + tsc + Vitest), `cargo clippy --all-targets -D warnings`,
  `cargo test --workspace` pass locally, and the change is demonstrated on
  `pnpm dev:mock` (fake device) or a fixture. Report failures verbatim;
  never claim "done" without running the checks.
- **No surprises in dependencies.** Adding a dependency requires: license
  compatible (MIT/Apache/BSD/MPL; LGPL only as a separate sidecar process,
  never linked), maintained in the last 12 months, named in the ADR.
- **Security and privacy.** No telemetry, no network calls except explicit
  device connections and the opt-in update check. Device identifiers,
  serials and logs stay on disk under the user's app-data folder; exports go
  only where the user chose. Never log secrets from device logs to console.
- **Cross-platform always.** Every core change compiles on macOS and Windows.
  Platform-specific code lives behind a trait/interface with a clear name;
  no `#[cfg]` scattered through business logic.
- **Design rules.** Use tokens, never literal colors in components. Numbers
  in the mono face with tabular figures. Every value carries its unit.
  Status is color + label. Charts are drawn to scale with real axes.
- **React Compiler is on.** It memoizes on the dependencies it can see in a
  callback body and ignores manual `useMemo`/`useEffect` dependency lists.
  Data that lives outside React (the telemetry mirror) must be read through
  `useTelemetryValue` / `useCores` / `useProcesses` / `useMetricSeries` in
  `apps/desktop/src/hooks/useDevice.ts`, never through a `useMemo` keyed on
  an invalidation counter. Tests must dispatch data both before and after
  render to catch this.
- **Two React copies break hooks.** `packages/ui` lists react as a peer;
  Vite dedupes `react`/`react-dom` in `apps/desktop/vite.config.ts`. Keep it.

## Stack (decided; see docs/DECISIONS.md and docs/research/stack.md)
Tauri 2 + Rust core (tokio, `idevice` for iOS, `adb_client` + adb binary
for Android, rusqlite) and a React 19 + TypeScript UI (Vite, Zustand,
Tailwind v4 tokens, uPlot charts, react-virtuoso log). Fonts bundled.
The window is opaque; never enable Tauri window transparency.

## Sub-agent protocol
When the lead (this session) delegates, each sub-agent receives:
`Task`, `Phase/Step`, `Files it may touch`, `Definition of done`,
`How to verify`. The agent must:
1. Read the four docs above first and only the code it needs.
2. Stay inside the allowed files; propose, don't perform, changes elsewhere.
3. Run the verification commands and paste the exact output tail.
4. Return a report: what changed (file list), what was verified, what is
   unverified or blocked, any decision that needs an ADR, next suggested step.
5. Never modify `docs/PLAN.md` phase order; mark steps done with a date.
Parallel agents get disjoint file sets. Shared files (`protocol`, `tokens`)
are changed only by the lead or by one agent at a time.

## Agent roles used in this project
- **core-ios**: Rust provider for iOS/iPadOS (lockdown, diagnostics, DVT,
  logs, tunnel). Fixtures from real devices.
- **core-android**: Rust provider for Android over ADB.
- **core-store**: SQLite schema, migrations, retention, export.
- **ui**: React components, screens, charts; owns `apps/desktop/src`.
- **protocol**: schema and codegen; consulted by everyone.
- **qa**: tests, fixtures, fake device scripts, Playwright flows, perf checks.
- **release**: CI, signing, notarization, updater, packaging.
The lead session integrates, reviews every diff, and keeps the plan current.

## Definition of done for any feature
Schema + fake + provider(s) + UI + tests + docs updated + verified on the
fake device + verified on at least one real device per platform it claims.

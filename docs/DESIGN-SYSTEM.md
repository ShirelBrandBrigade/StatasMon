# DeviceLog design system (from direction AD-1 "Dark Glass")

Source of truth for every screen. Values are lifted from the chosen mockup
(`docs/design-reference/dark-glass.css`, `docs/design-reference/dark-glass.body.html`; all mockups in `design-mockups/`). Do not invent new colors, radii or
type sizes; extend by composing these tokens.

## Tone
Dark instrument panel with soft glass cards. Calm by default; color means
state. One plain-language headline tells the user what matters right now,
then the numbers back it up. Dense inside cards, generous between them.

## Color tokens (CSS custom properties)

| Token | Value | Use |
|---|---|---|
| `--bg` | `#0f1219` | app ground |
| `--mesh-1/2/3` | `rgba(90,208,230,.14)`, `rgba(160,140,240,.14)`, `rgba(111,211,154,.10)` | three radial glows behind the ground; static, no animation |
| `--card` | `rgba(255,255,255,.05)` | card fill (over `backdrop-filter: blur(18px)`) |
| `--edge` | `rgba(255,255,255,.09)` | card and control borders |
| `--fg` | `#e6e9f2` | primary text |
| `--mut` | `#8b93a7` | secondary text |
| `--dim` | `#5c6478` | tertiary text, axis labels, disabled |
| `--cyan` | `#5ad0e6` | primary data series (CPU, download), primary action |
| `--cyan-2` | `#3d8fa0` | efficiency cores, secondary cyan |
| `--vio` | `#a08cf0` | second series (memory wired, upload) |
| `--mint` | `#6fd39a` | third series (storage), "good" state |
| `--amber` | `#f0b64a` | fourth series (battery), "warning" state |
| `--red` | `#ef6f6f` | "serious/critical" state, faults, recording dot |

Rules
- Series colors are assigned in fixed order cyan → violet → mint → amber and
  never cycled; a fifth series is "Other" in `rgba(255,255,255,.12)`.
- Status colors (mint good, amber warning, red serious/critical) always pair
  with a label or icon, never color alone.
- Text is never painted in a series color; a colored mark sits beside it.
- Selected/active controls invert: fill `--fg`, text `--bg`.
- Tinted pills: `rgba(<series>,.16)` fill with the series color as text.

## Typography
- UI face: **Manrope** 500/600/700/800 (bundled locally, not fetched).
- Data face: **JetBrains Mono** 400/500/600/700 with `font-variant-numeric:
  tabular-nums`, for every number, timestamp, identifier and log line.
- Fallbacks: `system-ui, sans-serif` and `ui-monospace, Menlo, Consolas, monospace`.

| Role | Size / weight |
|---|---|
| Headline (status sentence) | 24px / 800, letter-spacing −.02em |
| Headline support text | 13.5px / 500, `--mut` |
| Card title | 12.5px / 700 |
| Card subtitle | 11.5px / 500, `--dim` |
| Big figure | 22–24px mono / 600 |
| Body / table | 12–13px |
| Log line | 11.5px mono |
| Section label | 11px / 700, uppercase, letter-spacing .1em, `--dim` |

## Shape and depth
- Card radius 18px; controls 11px; pills 999px; app-icon squares 6px.
- Card shadow `0 12px 30px rgba(0,0,0,.2)`; primary button glow
  `0 6px 18px rgba(90,208,230,.3)`.
- Gaps: 14px between cards, 22–26px page padding, 9px inside-card stack gap.
- Sidebar 250px, right border `--edge`, `rgba(255,255,255,.025)` fill.

## Components (names used in code)
- **Sidebar**: brand, Devices list (DeviceCard: icon tile 36px, name, transport ·
  state, status dot), "+ Add a device", Views nav (icon + label, active = card
  fill), RecordingCard pinned to the bottom (red dot, elapsed, entry count,
  Pause / Mark).
- **Headline**: one sentence generated from state ("iPhone 16 Pro is running a
  little warm") plus one supporting sentence with the cause; right side:
  RangeSegment (Live · 5 min · 15 min · 1 h · Session) and Export button.
- **MetricCard** with a **Ring** (78px, stroke 8, track `rgba(255,255,255,.08)`)
  and a right-hand detail block: CoreBars (6 bars), StackBar (8px, 2px gaps) +
  Legend, or a KeyValue list.
- **NetworkCard**: two big figures (down cyan, up violet), packets/s and totals
  in mono, area chart with gradient fill, endpoint dot with `--bg` stroke.
- **ThermalCard**: state pill + sensor KeyValue list + sparkline.
- **TopAppsCard**: rows with app tile, name, bar, mono percent.
- **LogCard**: filter pills (apps, levels), rows [app tile · time · process ·
  level pill · message], fault rows highlighted `rgba(239,111,111,.1)` with a
  1px red outline, footer with counts and Export.
- **Charts**: 2px lines, gradient area at .35→0 alpha, grid lines
  `rgba(255,255,255,.06)`, no axis boxes, time labels in `--dim` mono 10.5px.

## Motion
- One reveal on connect: cards fade/slide in over 240ms, staggered 30ms.
- Live values update without animation; charts append, never redraw with easing.
- Respect `prefers-reduced-motion`: disable the reveal.

## Screen inventory (1440×900 reference, min window 1180×720)
Overview (the mockup) · Performance (CPU per core, memory, thermal, battery
with full history) · Network (interfaces, throughput, packets, per-process,
capture) · Apps & processes (table, per-app drill-in) · Logs (full-height
log with filters, per-app tabs) · Sessions (recorded sessions, compare,
export) · Device (full device information sheet) · Settings.

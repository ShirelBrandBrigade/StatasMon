import "uplot/dist/uPlot.min.css";
import "./chart.css";
import { toneColor } from "@statasmon/ui";
import { type CSSProperties, useEffect, useRef } from "react";
import uPlot from "uplot";
import { resolveCssColor, resolveToken, withAlpha } from "./cssColor";
import { type ChartData, toAlignedData } from "./useSeriesData";

export interface ChartSeries {
  id: string;
  label: string;
  /** A token reference such as `var(--cyan)`; resolved once at mount. */
  color: string;
  /** Gradient area under the line (series color at .35 → 0 alpha). */
  fill?: boolean;
  /** Mark the newest point with a dot outlined in `--bg`. */
  endpoint?: boolean;
}

export type MarkerTone = "info" | "warn" | "bad" | "neutral";

/** A mark or event drawn as a 1px vertical line with a label chip (docs/PLAN.md 5.3). */
export interface ChartMarker {
  tsMs: number;
  label: string;
  tone: MarkerTone;
}

export interface TimeSeriesChartProps {
  series: ChartSeries[];
  /** Marks and events; at most `MAX_MARKERS` (the newest in view) are drawn. */
  markers?: ChartMarker[];
  /** Formats x-axis labels and the tooltip time; defaults to the local wall clock. */
  xFormat?: (sec: number, withSeconds: boolean) => string;
  /** Timestamps in ms plus one value column per series (NaN = gap). Arrays may be reused between updates. */
  data: ChartData;
  height: number;
  /** Formats y values for the axis and the tooltip; include the unit. */
  yFormat?: (value: number) => string;
  /** Visible x range: [newest − windowMs, newest]. Undefined = all data (session). */
  windowMs?: number;
  /** Lower bound of the y axis; "auto" follows the data. */
  yMin?: number | "auto";
  /** Unit of the y values, for the accessible description. */
  unit?: string;
  /** Accessible name. */
  name?: string;
  /** Crosshair + hover tooltip; on by default. */
  hover?: boolean;
  /** Receives the uPlot instance (tests, perf harness). */
  instanceRef?: { current: uPlot | null };
  className?: string;
  style?: CSSProperties;
}

const LINE_WIDTH = 2;
const FILL_ALPHA_TOP = 0.42;
const FILL_ALPHA_BOTTOM = 0;
const DOT_RADIUS = 4;
const DOT_STROKE = 2;
const HOVER_POINT_SIZE = 7;
const PAD_TOP = 6;
const PAD_RIGHT = 8;
const X_AXIS_SIZE = 18;
const Y_AXIS_MIN_SIZE = 32;
const Y_AXIS_GAP = 6;
const Y_HEADROOM = 1.15;
const EMPTY_SPAN_S = 60;
/**
 * A window is not shown wider than the data it holds (plus this floor), so a
 * fresh session fills the plot from the left instead of squeezing thirty
 * seconds of samples into the right edge of a five-minute grid.
 */
export const MIN_SPAN_S = 30;
/** A gap longer than this many typical sample intervals (and at least GAP_BREAK_MIN_S) breaks the curve. */
const GAP_BREAK_FACTOR = 4;
const GAP_BREAK_MIN_S = 5;
/** Soft halo under every line (canvas px at 1×, alpha). */
const GLOW_WIDTH = 7;
const GLOW_ALPHA = 0.22;
const TIP_GAP = 10;
/** Markers drawn at most per chart (the newest ones inside the visible range). */
export const MAX_MARKERS = 12;
/** Cursor distance (CSS px) within which the tooltip names the nearest marker. */
const MARKER_TIP_PX = 6;
const MARKER_CHIP_PAD = 4;
/** Widest label chip (CSS px) before the text is cut with an ellipsis. */
const MARKER_CHIP_MAX_W = 140;
const MARKER_CHIP_H = 14;
const MARKER_CHIP_RADIUS = 4;
const MARKER_CHIP_ALPHA = 0.18;
const MARKER_TONES: readonly MarkerTone[] = ["info", "warn", "bad", "neutral"];
const NO_MARKERS: ChartMarker[] = [];

interface Theme {
  dim: string;
  grid: string;
  bg: string;
  font: string;
}

interface Live {
  data: ChartData;
  windowMs: number | undefined;
  yFormat: (value: number) => string;
  xFormat: (sec: number, withSeconds: boolean) => string;
  yMin: number | "auto";
  series: ChartSeries[];
  markers: ChartMarker[];
}

interface Tooltip {
  root: HTMLDivElement;
  time: HTMLDivElement;
  values: HTMLSpanElement[];
  marker: HTMLDivElement;
}

const pad2 = (n: number) => String(n).padStart(2, "0");

/** Local wall-clock label for a timestamp in seconds: HH:MM, or HH:MM:SS. */
export function formatClock(sec: number, withSeconds = false): string {
  const d = new Date(sec * 1000);
  const hm = `${pad2(d.getHours())}:${pad2(d.getMinutes())}`;
  return withSeconds ? `${hm}:${pad2(d.getSeconds())}` : hm;
}

/** Elapsed-time label for an x value in seconds since a session start: `m:ss` or `h:mm:ss`. */
export function formatElapsedAxis(sec: number, withSeconds = true): string {
  const total = Math.max(0, Math.round(sec));
  const h = Math.floor(total / 3600);
  const m = Math.floor((total % 3600) / 60);
  const s = total % 60;
  if (!withSeconds && h === 0) return `${m}:${pad2(s)}`;
  return h > 0 ? `${h}:${pad2(m)}:${pad2(s)}` : `${m}:${pad2(s)}`;
}

function defaultFormat(value: number): string {
  return value.toLocaleString("en-US", { maximumFractionDigits: 1 });
}

/** Axis + tooltip styling resolved from tokens at mount (canvas cannot read `var()`). */
function readTheme(el: Element): Theme {
  const mono = resolveToken("--font-mono", el) || "ui-monospace, monospace";
  const size = resolveToken("--fs-axis", el) || "10.5px";
  return {
    dim: resolveToken("--dim", el),
    grid: resolveToken("--grid", el),
    bg: resolveToken("--bg", el),
    font: `${size} ${mono}`,
  };
}

function createTooltip(series: ChartSeries[], colors: string[]): Tooltip {
  const root = document.createElement("div");
  root.className = "sm-chart-tip";
  root.hidden = true;
  root.setAttribute("role", "status");
  const time = document.createElement("div");
  time.className = "sm-chart-tip-time mono";
  root.appendChild(time);
  const marker = document.createElement("div");
  marker.className = "sm-chart-tip-marker";
  marker.hidden = true;
  root.appendChild(marker);
  const values = series.map((s, i) => {
    const row = document.createElement("div");
    row.className = "sm-chart-tip-row";
    const mark = document.createElement("i");
    mark.style.background = colors[i];
    mark.setAttribute("aria-hidden", "true");
    const label = document.createElement("span");
    label.textContent = s.label;
    const value = document.createElement("span");
    value.className = "sm-chart-tip-value mono";
    row.append(mark, label, value);
    root.appendChild(row);
    return value;
  });
  return { root, time, values, marker };
}

/** The newest `MAX_MARKERS` markers inside the visible x range, oldest → newest. */
export function visibleMarkers(markers: readonly ChartMarker[], minSec: number, maxSec: number) {
  const inRange = markers.filter((m) => m.tsMs / 1000 >= minSec && m.tsMs / 1000 <= maxSec);
  inRange.sort((a, b) => a.tsMs - b.tsMs);
  return inRange.length > MAX_MARKERS ? inRange.slice(-MAX_MARKERS) : inRange;
}

function roundedRect(
  ctx: CanvasRenderingContext2D,
  x: number,
  y: number,
  w: number,
  h: number,
  r: number,
) {
  const rr = Math.min(r, w / 2, h / 2);
  ctx.beginPath();
  ctx.moveTo(x + rr, y);
  ctx.arcTo(x + w, y, x + w, y + h, rr);
  ctx.arcTo(x + w, y + h, x, y + h, rr);
  ctx.arcTo(x, y + h, x, y, rr);
  ctx.arcTo(x, y, x + w, y, rr);
  ctx.closePath();
}

/**
 * 1px vertical line per marker across the plot area plus a tinted label chip
 * at the top (mono, `--fs-axis`), flipped to the left of the line when it
 * would leave the plot. Colors are the tone tokens resolved at mount.
 */
/** `text` cut with an ellipsis so it measures at most `maxW` px in the current font. */
function fitText(ctx: CanvasRenderingContext2D, text: string, maxW: number): string {
  if (ctx.measureText(text).width <= maxW) return text;
  let lo = 0;
  let hi = text.length;
  while (lo < hi) {
    const mid = (lo + hi + 1) >> 1;
    if (ctx.measureText(`${text.slice(0, mid)}…`).width <= maxW) lo = mid;
    else hi = mid - 1;
  }
  return lo === 0 ? "…" : `${text.slice(0, lo).trimEnd()}…`;
}

function drawMarkers(
  u: uPlot,
  markers: readonly ChartMarker[],
  tones: Record<MarkerTone, string>,
  font: string,
) {
  if (markers.length === 0) return;
  const { min, max } = u.scales.x;
  if (min == null || max == null) return;
  const shown = visibleMarkers(markers, min, max);
  if (shown.length === 0) return;
  const pr = uPlot.pxRatio;
  const ctx = u.ctx;
  const { left, top, width, height } = u.bbox;
  ctx.save();
  ctx.font = font;
  ctx.textBaseline = "middle";
  ctx.textAlign = "left";
  const maxTextW = MARKER_CHIP_MAX_W * pr;
  // Chips already drawn, as [x0, x1] spans: a chip that would overlap one of
  // them is not drawn (its line still is), so clustered events stay legible.
  const taken: Array<[number, number]> = [];
  for (const m of shown) {
    const x = Math.round(u.valToPos(m.tsMs / 1000, "x", true));
    if (!Number.isFinite(x)) continue;
    const color = tones[m.tone];
    ctx.fillStyle = color;
    ctx.fillRect(x, top, Math.max(1, Math.round(pr)), height);
    const label = fitText(ctx, m.label, maxTextW);
    const textW = ctx.measureText(label).width;
    const w = textW + MARKER_CHIP_PAD * 2 * pr;
    const h = MARKER_CHIP_H * pr;
    const cx = x + w > left + width ? x - w - pr : x + pr;
    if (taken.some(([a, b]) => cx < b + pr && cx + w > a - pr)) continue;
    taken.push([cx, cx + w]);
    roundedRect(ctx, cx, top, w, h, MARKER_CHIP_RADIUS * pr);
    ctx.fillStyle = withAlpha(color, MARKER_CHIP_ALPHA);
    ctx.fill();
    ctx.fillStyle = color;
    ctx.fillText(label, cx + MARKER_CHIP_PAD * pr, top + h / 2);
  }
  ctx.restore();
}

/** The marker closest to the cursor (CSS px) when within `MARKER_TIP_PX`. */
function markerNear(
  u: uPlot,
  markers: readonly ChartMarker[],
  left: number,
): ChartMarker | undefined {
  const { min, max } = u.scales.x;
  if (min == null || max == null) return undefined;
  let best: ChartMarker | undefined;
  let bestDist = MARKER_TIP_PX;
  for (const m of visibleMarkers(markers, min, max)) {
    const d = Math.abs(u.valToPos(m.tsMs / 1000, "x") - left);
    if (d <= bestDist) {
      best = m;
      bestDist = d;
    }
  }
  return best;
}

/** Endpoint dots (radius 4, `--bg` stroke 2px) on the newest point of each flagged series. */
function drawEndpoints(u: uPlot, colors: string[], flags: boolean[], bg: string) {
  const pr = uPlot.pxRatio;
  const ctx = u.ctx;
  const xs = u.data[0];
  for (let i = 1; i < u.series.length; i += 1) {
    if (!flags[i - 1]) continue;
    const ys = u.data[i];
    let idx = ys.length - 1;
    while (idx >= 0 && ys[idx] == null) idx -= 1;
    if (idx < 0) continue;
    const x = u.valToPos(xs[idx] as number, "x", true);
    const y = u.valToPos(ys[idx] as number, "y", true);
    if (!Number.isFinite(x) || !Number.isFinite(y)) continue;
    ctx.save();
    ctx.beginPath();
    ctx.arc(x, y, DOT_RADIUS * pr, 0, Math.PI * 2);
    ctx.fillStyle = colors[i - 1];
    ctx.fill();
    ctx.lineWidth = DOT_STROKE * pr;
    ctx.strokeStyle = bg;
    ctx.stroke();
    ctx.restore();
  }
}

/**
 * Smooth, monotone (Fritsch–Carlson) cubic path through the visible points:
 * no overshoot past a sample, so a CPU spike never dips below 0 % or rings
 * above 100 %. Only a real outage breaks the line (see `GAP_BREAK_FACTOR`).
 * The fill closes to the zero line (or the bottom of the scale when zero is
 * out of range).
 */
export function monotonePaths(): uPlot.Series.PathBuilder {
  return (u, sidx, idx0, idx1) => {
    const xs = u.data[0];
    const ys = u.data[sidx];
    const scaleY = u.series[sidx].scale ?? "y";
    const yScale = u.scales[scaleY];
    const zero = Math.min(Math.max(0, yScale.min ?? 0), yScale.max ?? 0);
    const baseY = u.valToPos(zero, scaleY, true);
    const stroke = new Path2D();
    const fill = new Path2D();
    let seg: number[] = [];

    const flush = () => {
      const n = seg.length / 2;
      if (n === 0) return;
      const px = (i: number) => seg[i * 2];
      const py = (i: number) => seg[i * 2 + 1];
      if (n === 1) {
        stroke.moveTo(px(0), py(0));
        stroke.lineTo(px(0), py(0));
        seg = [];
        return;
      }
      // Secants and tangents.
      const d: number[] = [];
      for (let i = 0; i < n - 1; i += 1) {
        const h = px(i + 1) - px(i);
        d.push(h === 0 ? 0 : (py(i + 1) - py(i)) / h);
      }
      const m: number[] = [d[0]];
      for (let i = 1; i < n - 1; i += 1) {
        m.push(d[i - 1] * d[i] <= 0 ? 0 : (d[i - 1] + d[i]) / 2);
      }
      m.push(d[n - 2]);
      for (let i = 0; i < n - 1; i += 1) {
        if (d[i] === 0) {
          m[i] = 0;
          m[i + 1] = 0;
          continue;
        }
        const a = m[i] / d[i];
        const b = m[i + 1] / d[i];
        const q = a * a + b * b;
        if (q > 9) {
          const t = 3 / Math.sqrt(q);
          m[i] = t * a * d[i];
          m[i + 1] = t * b * d[i];
        }
      }
      stroke.moveTo(px(0), py(0));
      fill.moveTo(px(0), baseY);
      fill.lineTo(px(0), py(0));
      for (let i = 0; i < n - 1; i += 1) {
        const h = (px(i + 1) - px(i)) / 3;
        const c1x = px(i) + h;
        const c1y = py(i) + m[i] * h;
        const c2x = px(i + 1) - h;
        const c2y = py(i + 1) - m[i + 1] * h;
        stroke.bezierCurveTo(c1x, c1y, c2x, c2y, px(i + 1), py(i + 1));
        fill.bezierCurveTo(c1x, c1y, c2x, c2y, px(i + 1), py(i + 1));
      }
      fill.lineTo(px(n - 1), baseY);
      fill.closePath();
      seg = [];
    };

    // Real points only: a missing value at one tick (another series' tick,
    // a metric the provider skipped once) does not break the curve. A break
    // happens where the time between two samples is far longer than usual:
    // the device was gone, the session paused, or history is coarser.
    const px: number[] = [];
    const py: number[] = [];
    const pt: number[] = [];
    for (let i = idx0; i <= idx1; i += 1) {
      const y = ys[i];
      if (y == null) continue;
      pt.push(xs[i] as number);
      px.push(u.valToPos(xs[i] as number, "x", true));
      py.push(u.valToPos(y, scaleY, true));
    }
    const gaps: number[] = [];
    for (let i = 1; i < pt.length; i += 1) gaps.push(pt[i] - pt[i - 1]);
    const sorted = gaps.slice().sort((a, b) => a - b);
    const typical = sorted.length > 0 ? sorted[Math.floor(sorted.length / 2)] : 0;
    const breakAfter = Math.max(typical * GAP_BREAK_FACTOR, GAP_BREAK_MIN_S);
    for (let i = 0; i < pt.length; i += 1) {
      if (i > 0 && pt[i] - pt[i - 1] > breakAfter) flush();
      seg.push(px[i], py[i]);
    }
    flush();
    return { stroke, fill, clip: null, flags: 0 };
  };
}

function buildOptions(
  host: HTMLElement,
  height: number,
  hover: boolean,
  live: { current: Live },
  theme: Theme,
  colors: string[],
  tones: Record<MarkerTone, string>,
  tip: Tooltip | null,
): uPlot.Options {
  const series = live.current.series;
  const fills = series.map((s) => Boolean(s.fill));
  const endpoints = series.map((s) => Boolean(s.endpoint));
  const fmt = (v: number) => live.current.yFormat(v);
  const xfmt = (sec: number, withSeconds: boolean) => live.current.xFormat(sec, withSeconds);
  let ySize = Y_AXIS_MIN_SIZE;
  const gridLine = { show: true, stroke: theme.grid, width: 1 };
  const gridDashed = { show: true, stroke: theme.grid, width: 1, dash: [3, 5] };
  const noLine = { show: false };
  const smooth = monotonePaths();

  return {
    width: Math.max(1, Math.round(host.clientWidth) || 320),
    height,
    legend: { show: false },
    padding: [PAD_TOP, PAD_RIGHT, 0, 0],
    cursor: hover
      ? {
          x: true,
          y: true,
          drag: { x: false, y: false, setScale: false },
          focus: { prox: -1 },
          points: {
            size: HOVER_POINT_SIZE,
            width: DOT_STROKE,
            stroke: (_u, i) => colors[i - 1],
            fill: () => theme.bg,
          },
        }
      : { show: false },
    scales: {
      x: {
        time: true,
        range: (_u, min, max) => {
          const w = live.current.windowMs;
          if (!Number.isFinite(max)) {
            const now = Date.now() / 1000;
            return [now - (w ?? EMPTY_SPAN_S * 1000) / 1000, now];
          }
          if (w === undefined) {
            return Number.isFinite(min) && min < max ? [min, max] : [max - EMPTY_SPAN_S, max];
          }
          const full = w / 1000;
          const have = Number.isFinite(min) ? max - min : 0;
          const span = Math.min(full, Math.max(have, MIN_SPAN_S));
          return [max - span, max];
        },
      },
      y: {
        range: (_u, min, max) => {
          const top = Number.isFinite(max) && max > 0 ? max * Y_HEADROOM : 1;
          const yMin = live.current.yMin;
          if (yMin === "auto") {
            const bottom = Number.isFinite(min) ? Math.min(min, top) : 0;
            return [bottom === top ? bottom - 1 : bottom, top];
          }
          return [yMin, Math.max(top, yMin + 1)];
        },
      },
    },
    axes: [
      {
        stroke: theme.dim,
        font: theme.font,
        size: X_AXIS_SIZE,
        gap: 6,
        space: 96,
        ticks: noLine,
        border: noLine,
        grid: gridDashed,
        values: (_u, splits, _axisIdx, _foundSpace, foundIncr) =>
          splits.map((s) => xfmt(s, foundIncr < 60)),
      },
      {
        stroke: theme.dim,
        font: theme.font,
        gap: Y_AXIS_GAP,
        space: 40,
        ticks: noLine,
        border: noLine,
        grid: gridLine,
        values: (_u, splits) => splits.map((v) => fmt(v)),
        size: (u, values) => {
          // Measure every layout pass: the first pass sees short placeholder
          // splits, and a cached width clipped "33.4 °C" to "3.4 °C".
          if (!values || values.length === 0) return ySize;
          u.ctx.font = theme.font;
          let w = 0;
          for (const v of values) w = Math.max(w, u.ctx.measureText(String(v)).width);
          ySize = Math.max(ySize, Y_AXIS_MIN_SIZE, Math.ceil(w) + Y_AXIS_GAP + 8);
          return ySize;
        },
      },
    ],
    series: [
      {},
      ...series.map((s, i) => ({
        label: s.label,
        stroke: colors[i],
        width: LINE_WIDTH,
        spanGaps: false,
        paths: smooth,
        points: { show: false },
        fill: fills[i]
          ? (u: uPlot) => {
              const { top, height: h } = u.bbox;
              const g = u.ctx.createLinearGradient(0, top, 0, top + h);
              g.addColorStop(0, withAlpha(colors[i], FILL_ALPHA_TOP));
              g.addColorStop(1, withAlpha(colors[i], FILL_ALPHA_BOTTOM));
              return g;
            }
          : undefined,
      })),
    ],
    hooks: {
      // A soft halo under each line, drawn right after uPlot strokes it.
      drawSeries: [
        (u, sidx) => {
          const paths = (u.series[sidx] as { _paths?: { stroke?: Path2D | null } | null })._paths;
          const path = paths?.stroke;
          if (!path) return;
          const ctx = u.ctx;
          ctx.save();
          ctx.beginPath();
          ctx.rect(u.bbox.left, u.bbox.top, u.bbox.width, u.bbox.height);
          ctx.clip();
          ctx.strokeStyle = withAlpha(colors[sidx - 1], GLOW_ALPHA);
          ctx.lineWidth = GLOW_WIDTH * uPlot.pxRatio;
          ctx.lineJoin = "round";
          ctx.lineCap = "round";
          ctx.stroke(path);
          ctx.restore();
        },
      ],
      draw: [
        (u) => {
          drawEndpoints(u, colors, endpoints, theme.bg);
          drawMarkers(u, live.current.markers, tones, theme.font);
        },
      ],
      setCursor: tip
        ? [
            (u) => {
              const { idx, left } = u.cursor;
              const t = idx == null ? undefined : u.data[0][idx];
              if (idx == null || left == null || left < 0 || t == null) {
                tip.root.hidden = true;
                return;
              }
              tip.time.textContent = xfmt(t, true);
              for (let i = 0; i < tip.values.length; i += 1) {
                const v = u.data[i + 1]?.[idx];
                tip.values[i].textContent = v == null ? "—" : fmt(v);
              }
              const near = markerNear(u, live.current.markers, left);
              tip.marker.hidden = near === undefined;
              tip.marker.textContent = near ? near.label : "";
              tip.marker.dataset.tone = near ? near.tone : "";
              const w = u.over.clientWidth;
              if (w > 0 && left > w / 2) {
                tip.root.style.left = "";
                tip.root.style.right = `${Math.round(w - left + TIP_GAP)}px`;
              } else {
                tip.root.style.right = "";
                tip.root.style.left = `${Math.round(left + TIP_GAP)}px`;
              }
              tip.root.hidden = false;
            },
          ]
        : [],
    },
  };
}

/**
 * Streaming time-series chart on ONE uPlot instance per mount. Data updates go
 * through `setData` only (never a re-create, never an explicit redraw); the
 * scales re-range through their `range` functions. There is no animation, so
 * `prefers-reduced-motion` needs no special case.
 */
export function TimeSeriesChart({
  series,
  markers = NO_MARKERS,
  xFormat = formatClock,
  data,
  height,
  yFormat = defaultFormat,
  windowMs,
  yMin = 0,
  unit,
  name,
  hover = true,
  instanceRef,
  className,
  style,
}: TimeSeriesChartProps) {
  const hostRef = useRef<HTMLDivElement>(null);
  const plotRef = useRef<uPlot | null>(null);
  const alignedRef = useRef<uPlot.AlignedData | undefined>(undefined);
  const live = useRef<Live>({ data, windowMs, yFormat, xFormat, yMin, series, markers });

  // Latest props for uPlot callbacks; declared first so it runs before the effects below.
  useEffect(() => {
    live.current = { data, windowMs, yFormat, xFormat, yMin, series, markers };
  });

  const seriesKey = series
    .map((s) => `${s.id}|${s.label}|${s.color}|${s.fill ? 1 : 0}|${s.endpoint ? 1 : 0}`)
    .join(";");

  // Create once per (series layout, height, hover); destroy on unmount.
  // biome-ignore lint/correctness/useExhaustiveDependencies: `seriesKey` is the content key for `series` (read through `live`), so an equal array does not re-create the chart.
  useEffect(() => {
    const host = hostRef.current;
    if (!host) return undefined;
    const theme = readTheme(host);
    const current = live.current.series;
    const colors = current.map((s) => resolveCssColor(s.color, host));
    const tones = Object.fromEntries(
      MARKER_TONES.map((t) => [t, resolveCssColor(toneColor(t), host)]),
    ) as Record<MarkerTone, string>;
    const tip = hover ? createTooltip(current, colors) : null;
    const opts = buildOptions(host, height, hover, live, theme, colors, tones, tip);
    const u = new uPlot(opts, toAlignedData(live.current.data, alignedRef.current), host);
    alignedRef.current = u.data;
    if (tip) u.over.appendChild(tip.root);
    plotRef.current = u;
    if (instanceRef) instanceRef.current = u;

    let ro: ResizeObserver | undefined;
    if (typeof ResizeObserver !== "undefined") {
      ro = new ResizeObserver((entries) => {
        const w = Math.round(entries[0]?.contentRect.width ?? 0);
        if (w > 0 && w !== u.width) u.setSize({ width: w, height });
      });
      ro.observe(host);
    }
    return () => {
      ro?.disconnect();
      u.destroy();
      plotRef.current = null;
      if (instanceRef) instanceRef.current = null;
    };
  }, [seriesKey, height, hover, instanceRef]);

  // Data and window updates: setData only (the x range function reads `windowMs` from `live`).
  useEffect(() => {
    const u = plotRef.current;
    if (!u) return;
    live.current.windowMs = windowMs;
    alignedRef.current = toAlignedData(data, alignedRef.current);
    u.setData(alignedRef.current);
    const host = hostRef.current;
    if (host) host.dataset.empty = alignedRef.current[0].length === 0 ? "true" : "false";
  }, [data, windowMs]);

  // Markers change rarely (a mark or an event); repaint without rebuilding paths.
  // The mount-time draw already includes them, so the first run is skipped.
  const markersSeen = useRef(markers);
  useEffect(() => {
    if (markersSeen.current === markers) return;
    markersSeen.current = markers;
    plotRef.current?.redraw(false);
  }, [markers]);

  const label = name ?? `${series.map((s) => s.label).join(" and ")}${unit ? ` in ${unit}` : ""}`;
  return (
    <div
      ref={hostRef}
      className={className ? `sm-chart ${className}` : "sm-chart"}
      role="img"
      aria-label={label}
      data-empty={data.t.length === 0 ? "true" : "false"}
      style={{ height, ...style }}
    />
  );
}

/**
 * Minimal browser stubs so uPlot can mount under jsdom without a canvas
 * package: a recording 2D context (no rasterization), `Path2D`, and
 * `matchMedia` (uPlot queries it at module load, so call `installCanvasStub()`
 * BEFORE importing anything that imports "uplot" — use a dynamic import).
 */

export interface CanvasStub {
  /** Names of every context method called, in order (e.g. "arc", "stroke"). */
  calls: string[];
  reset(): void;
}

const CONTEXT_METHODS = [
  "save",
  "restore",
  "beginPath",
  "closePath",
  "moveTo",
  "lineTo",
  "arc",
  "arcTo",
  "rect",
  "clip",
  "fill",
  "stroke",
  "fillRect",
  "strokeRect",
  "clearRect",
  "fillText",
  "strokeText",
  "translate",
  "scale",
  "rotate",
  "setLineDash",
  "setTransform",
  "resetTransform",
  "drawImage",
] as const;

class Path2DStub {
  addPath(): void {}
  moveTo(): void {}
  lineTo(): void {}
  bezierCurveTo(): void {}
  quadraticCurveTo(): void {}
  rect(): void {}
  arc(): void {}
  arcTo(): void {}
  closePath(): void {}
}

export function installCanvasStub(): CanvasStub {
  const calls: string[] = [];
  const ctx: Record<string, unknown> = {
    font: "",
    fillStyle: "",
    strokeStyle: "",
    lineWidth: 1,
    lineJoin: "miter",
    lineCap: "butt",
    textAlign: "left",
    textBaseline: "alphabetic",
    globalAlpha: 1,
    measureText: (text: string) => ({ width: text.length * 6 }),
    createLinearGradient: () => ({ addColorStop: () => {} }),
    createPattern: () => null,
    getLineDash: () => [],
    getImageData: () => ({ data: new Uint8ClampedArray(4) }),
  };
  for (const m of CONTEXT_METHODS) {
    ctx[m] = () => {
      calls.push(m);
    };
  }
  HTMLCanvasElement.prototype.getContext = (() =>
    ctx) as unknown as typeof HTMLCanvasElement.prototype.getContext;

  const g = globalThis as { Path2D?: unknown; matchMedia?: unknown };
  if (typeof g.Path2D === "undefined") g.Path2D = Path2DStub;
  if (typeof g.matchMedia === "undefined") {
    g.matchMedia = (query: string) => ({
      matches: false,
      media: query,
      onchange: null,
      addEventListener: () => {},
      removeEventListener: () => {},
      addListener: () => {},
      removeListener: () => {},
      dispatchEvent: () => false,
    });
  }
  return {
    calls,
    reset: () => {
      calls.length = 0;
    },
  };
}

/** Sets every `--token: value` of a CSS file inline on `<html>` so getComputedStyle resolves them under jsdom. */
export function applyTokens(css: string, el: HTMLElement = document.documentElement): void {
  for (const m of css.matchAll(/(--[\w-]+)\s*:\s*([^;]+);/g)) {
    el.style.setProperty(m[1], m[2].replace(/\s+/g, " ").trim());
  }
}

import "../../../styles.css";
import { Button, Segment } from "@statasmon/ui";
import React, { useEffect, useState } from "react";
import ReactDOM from "react-dom/client";
import { startBridge } from "../../../bridge";
import { type ChartPerfResult, runChartPerf } from "../../../charts/perfHarness";
import { useActiveDevice } from "../../../hooks/useDevice";
import { type Range, useAppStore } from "../../../store/app";
import { NetworkCard } from "../NetworkCard";

/**
 * Review page for step 1.3 until the Overview screen routes the card:
 * `pnpm web:mock`, then open /src/overview/network/dev/index.html. Replays
 * the fake fixture through the real bridge and store, so the card behaves
 * exactly as it will in the app. "Run chart perf" executes the in-browser
 * budget check from charts/perfHarness.ts.
 */

// Other files change while this page is under review; keep the page alive across
// full reloads (Vite's documented opt-out: throw from the listener).
if (import.meta.hot) {
  import.meta.hot.on("vite:beforeFullReload", () => {
    throw "[network dev] full reload skipped";
  });
}

const RANGES: ReadonlyArray<{ value: Range; label: string }> = [
  { value: "live", label: "Live" },
  { value: "5m", label: "5 min" },
  { value: "15m", label: "15 min" },
  { value: "1h", label: "1 h" },
  { value: "session", label: "Session" },
];

function DevPage() {
  const active = useActiveDevice();
  const order = useAppStore((s) => s.order);
  const setActive = useAppStore((s) => s.setActive);
  const range = useAppStore((s) => s.range);
  const setRange = useAppStore((s) => s.setRange);
  const [perf, setPerf] = useState<ChartPerfResult | "running" | undefined>(undefined);

  useEffect(() => {
    let stop: (() => void) | undefined;
    let cancelled = false;
    void startBridge().then((s) => {
      if (cancelled) s();
      else stop = s;
    });
    return () => {
      cancelled = true;
      stop?.();
    };
  }, []);

  return (
    <main
      className="app-mesh"
      style={{ minHeight: "100vh", padding: "var(--pad-page-y) var(--pad-page-x)" }}
    >
      <div
        style={{
          display: "flex",
          gap: 14,
          alignItems: "center",
          marginBottom: 14,
          flexWrap: "wrap",
        }}
      >
        <span className="section-label">Network card · dev harness</span>
        {order.map((id) => (
          <Button
            key={id}
            size="sm"
            variant={active?.summary.id === id ? "primary" : "ghost"}
            onClick={() => setActive(id)}
          >
            {id}
          </Button>
        ))}
        <Segment name="Range" options={RANGES} value={range} onChange={setRange} />
        <Button
          size="sm"
          variant="ghost"
          disabled={perf === "running"}
          onClick={() => {
            setPerf("running");
            void runChartPerf().then(setPerf);
          }}
        >
          Run chart perf
        </Button>
        <span
          className="mono"
          data-testid="perf-result"
          style={{ fontSize: "var(--fs-body)", color: "var(--mut)" }}
        >
          {perf === undefined
            ? ""
            : perf === "running"
              ? "running…"
              : `${perf.msPerUpdate.toFixed(2)} ms/update (max ${perf.maxMs.toFixed(2)} ms) · ${perf.points} pts × 2 · ${perf.updates} updates`}
        </span>
      </div>
      <div style={{ width: 640 }}>
        {active ? (
          <NetworkCard deviceId={active.summary.id} />
        ) : (
          <p style={{ color: "var(--mut)" }}>Waiting for a device…</p>
        )}
      </div>
    </main>
  );
}

const root = document.getElementById("root");
if (!root) throw new Error("#root missing");
ReactDOM.createRoot(root).render(
  <React.StrictMode>
    <DevPage />
  </React.StrictMode>,
);

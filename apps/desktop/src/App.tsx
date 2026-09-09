import { formatValue } from "@statasmon/protocol";
import { useEffect, useState } from "react";
import { getStream, startBridge } from "./bridge";
import { dispatchedCount } from "./bridge/dispatch";
import { Gallery } from "./gallery/Gallery";
import { useAppStore } from "./store/app";
import { telemetry } from "./store/telemetry";

const DEBUG_METRICS = [
  "cpu.total",
  "mem.used",
  "thermal.state",
  "bat.percent",
  "net.rx_bps",
] as const;

/**
 * Step 0.5 debug panel: proves the stream reaches the UI. Phase 1 replaces
 * this with the Overview screen; the Gallery stays reachable via `#gallery`.
 */
export function App() {
  const [hash, setHash] = useState(() => window.location.hash);
  useEffect(() => {
    const onHash = () => setHash(window.location.hash);
    window.addEventListener("hashchange", onHash);
    return () => window.removeEventListener("hashchange", onHash);
  }, []);
  if (hash === "#gallery") return <Gallery />;
  return <DebugPanel />;
}

function DebugPanel() {
  const devices = useAppStore((s) => s.devices);
  const order = useAppStore((s) => s.order);
  const streamKind = useAppStore((s) => s.streamKind);
  const [tick, setTick] = useState(0);

  useEffect(() => {
    let stop: (() => void) | undefined;
    void startBridge().then((s) => {
      stop = s;
    });
    const report = setTimeout(() => {
      void getStream().then((st) => {
        if (st.kind !== "tauri") return;
        const ids = Object.keys(useAppStore.getState().devices);
        void st.invoke("ui_report", {
          message: `ui received ${dispatchedCount} messages; devices=${ids.join(",")}`,
        });
      });
    }, 4000);
    const unsub = telemetry.subscribe((_id, kind) => {
      if (kind === "log") setTick((t) => t + 1);
    });
    return () => {
      clearTimeout(report);
      unsub();
      stop?.();
    };
  }, []);

  return (
    <main
      className="app-mesh"
      style={{ minHeight: "100vh", padding: 24, fontFamily: "var(--font-ui)" }}
    >
      <h1 style={{ margin: "0 0 4px", fontSize: 20 }}>StatasMon</h1>
      <p style={{ color: "var(--mut)", margin: "0 0 18px", fontSize: 13 }}>
        Debug panel (step 0.5) · stream: <span className="mono">{streamKind}</span> ·{" "}
        <a href="#gallery" style={{ color: "var(--cyan)" }}>
          primitives gallery
        </a>
      </p>
      <div style={{ display: "grid", gridTemplateColumns: "repeat(3, minmax(0, 1fr))", gap: 14 }}>
        {order.map((id) => {
          const d = devices[id];
          const log = telemetry.logsFor(id).all();
          return (
            <section
              key={id}
              className="glass"
              style={{ padding: 16, borderRadius: "var(--r-card)" }}
              data-testid={`device-${id}`}
            >
              <div
                style={{ display: "flex", justifyContent: "space-between", alignItems: "baseline" }}
              >
                <b>{d.summary.model_name}</b>
                <span className="mono" style={{ fontSize: 11, color: "var(--dim)" }}>
                  {d.summary.state.state} · {d.session?.status ?? "—"}
                </span>
              </div>
              <div style={{ fontSize: 13, color: "var(--mut)", margin: "4px 0 10px" }}>
                {d.headline?.sentence ?? "Waiting for headline…"}
              </div>
              <dl
                style={{
                  display: "grid",
                  gridTemplateColumns: "auto 1fr",
                  gap: "2px 12px",
                  margin: 0,
                  fontSize: 12,
                }}
              >
                {DEBUG_METRICS.map((m) => (
                  <MetricRow key={m} id={m} value={d.latest[m]} />
                ))}
              </dl>
              <div
                className="mono"
                style={{ fontSize: 11, marginTop: 10, color: "var(--dim)" }}
                data-tick={tick}
              >
                {log.length} log lines · {d.eventCount} events
                {log.slice(-3).map((e) => (
                  <div
                    key={e.seq}
                    style={{
                      color: "var(--mut)",
                      overflow: "hidden",
                      textOverflow: "ellipsis",
                      whiteSpace: "nowrap",
                    }}
                  >
                    {e.process} · {e.level} · {e.message}
                  </div>
                ))}
              </div>
            </section>
          );
        })}
      </div>
    </main>
  );
}

const UNITS = {
  "cpu.total": "percent",
  "mem.used": "bytes",
  "thermal.state": "state",
  "bat.percent": "percent",
  "net.rx_bps": "bits_per_sec",
} as const;

function MetricRow({ id, value }: { id: keyof typeof UNITS; value: number | undefined }) {
  return (
    <>
      <dt style={{ color: "var(--dim)" }}>{id}</dt>
      <dd className="mono" style={{ margin: 0 }}>
        {formatValue({ unit: UNITS[id] }, value)}
      </dd>
    </>
  );
}

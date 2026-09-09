// Dev-only harness for the log card: `pnpm web` then open
// http://localhost:1420/src/overview/log/dev/index.html
// Replays fixtures/fake/session-3dev-30s.jsonl through the bridge (looping),
// exactly like the app does in the browser. Not part of the production build.
import React from "react";
import ReactDOM from "react-dom/client";
import "../../../styles.css";
import { startBridge } from "../../../bridge";
import { useAppStore } from "../../../store/app";
import { LogCard } from "../LogCard";

function Harness() {
  const order = useAppStore((s) => s.order);
  const active = useAppStore((s) => s.activeDeviceId);
  const setActive = useAppStore((s) => s.setActive);
  return (
    <main
      className="app-mesh"
      style={{
        minHeight: "100vh",
        padding: "var(--pad-page-y) var(--pad-page-x)",
        display: "flex",
        flexDirection: "column",
        gap: 14,
        boxSizing: "border-box",
      }}
    >
      <div style={{ display: "flex", gap: 8, alignItems: "center" }}>
        <span className="section-label">Log card · dev harness</span>
        {order.map((id) => (
          <button
            key={id}
            type="button"
            className="log-pill"
            aria-pressed={id === active}
            onClick={() => setActive(id)}
          >
            {id}
          </button>
        ))}
      </div>
      {active ? (
        <LogCard deviceId={active} style={{ height: 460, flex: "0 0 auto" }} />
      ) : (
        <p style={{ color: "var(--mut)" }}>Waiting for a device…</p>
      )}
    </main>
  );
}

// Started once, outside React: the replay stream dedupes listeners in a Set,
// so StrictMode's mount-unmount-mount would otherwise unsubscribe `dispatch`.
void startBridge();

const root = document.getElementById("root");
if (!root) throw new Error("#root missing");
ReactDOM.createRoot(root).render(
  <React.StrictMode>
    <Harness />
  </React.StrictMode>,
);

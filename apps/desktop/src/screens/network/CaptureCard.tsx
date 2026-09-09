import { Button, Card, Dash, Pill } from "@statasmon/ui";
import { useEffect, useId, useState } from "react";
import { commands } from "../../bridge/commands";
import { isTauri } from "../../bridge/stream";
import { useCapability, useDevice } from "../../hooks/useDevice";
import { formatTime } from "../performance/shared";
import { defaultCapturePath, describeActionError, IDLE, useCaptureStore } from "./captureStore";
import { why } from "./why";

const INPUT_CLASS =
  "mono box-border h-8 min-h-8 w-full min-w-0 rounded-ctl border border-edge bg-card px-2.5 text-body text-fg outline-none placeholder:text-dim focus-visible:outline-2 focus-visible:outline-cyan focus-visible:-outline-offset-1 disabled:opacity-50";

/** Opens the native save sheet inside Tauri; resolves to undefined when cancelled or in a browser. */
async function chooseSavePath(defaultPath: string): Promise<string | undefined> {
  if (!isTauri()) return undefined;
  const { save } = await import("@tauri-apps/plugin-dialog");
  const picked = await save({
    defaultPath,
    filters: [{ name: "Packet capture", extensions: ["pcap"] }],
  });
  return picked ?? undefined;
}

/**
 * Packet capture (docs/FEATURES.md §7): start/stop `.pcap` capture through
 * `device_action`, the target path prefilled under ~/Downloads/StatasMon,
 * state as a pill, errors inline with their code and hint.
 */
export function CaptureCard({ deviceId }: { deviceId: string }) {
  const summary = useDevice(deviceId)?.summary;
  const capability = useCapability(deviceId, "packet_capture");
  const state = useCaptureStore((s) => s.byDevice[deviceId] ?? IDLE);
  const error = useCaptureStore((s) => s.errors[deviceId]);
  const storedPath = useCaptureStore((s) => s.paths[deviceId]);
  const setState = useCaptureStore((s) => s.set);
  const setError = useCaptureStore((s) => s.setError);
  const setPath = useCaptureStore((s) => s.setPath);
  const inputId = useId();
  const errorId = useId();
  const [now, setNow] = useState(() => Date.now());
  const tauri = isTauri();

  const path = storedPath ?? defaultCapturePath(summary?.name ?? deviceId);
  const unavailable = capability.available === false;
  const busy = state.kind === "starting" || state.kind === "stopping";
  const capturing = state.kind === "capturing" || state.kind === "stopping";

  // Elapsed time ticks once a second while a capture runs.
  useEffect(() => {
    if (!capturing) return undefined;
    const id = setInterval(() => setNow(Date.now()), 1000);
    return () => clearInterval(id);
  }, [capturing]);

  const start = async () => {
    const target = path.trim();
    setError(deviceId, undefined);
    setState(deviceId, { kind: "starting", path: target });
    try {
      await commands.deviceAction(deviceId, { action: "pcap_start", path: target });
      setState(deviceId, { kind: "capturing", path: target, sinceMs: Date.now() });
      setNow(Date.now());
    } catch (e) {
      setError(deviceId, describeActionError(e));
      setState(deviceId, { kind: "idle" });
    }
  };

  const stop = async () => {
    if (state.kind !== "capturing") return;
    setError(deviceId, undefined);
    setState(deviceId, { kind: "stopping", path: state.path, sinceMs: state.sinceMs });
    try {
      await commands.deviceAction(deviceId, { action: "pcap_stop" });
      setState(deviceId, { kind: "idle" });
    } catch (e) {
      setError(deviceId, describeActionError(e));
      setState(deviceId, { kind: "capturing", path: state.path, sinceMs: state.sinceMs });
    }
  };

  const choose = async () => {
    const picked = await chooseSavePath(path);
    if (picked) setPath(deviceId, picked);
  };

  const pill = unavailable ? (
    <span data-testid="pcap-state">
      <Dash reason={why(capability)} />
    </span>
  ) : state.kind === "capturing" || state.kind === "stopping" ? (
    <Pill tone="bad" data-testid="pcap-state">
      Capturing since <span className="mono">{formatTime(state.sinceMs)}</span>
    </Pill>
  ) : state.kind === "starting" ? (
    <Pill tone="warn" data-testid="pcap-state">
      Starting
    </Pill>
  ) : (
    <Pill tone="neutral" data-testid="pcap-state">
      Idle
    </Pill>
  );

  const elapsed =
    state.kind === "capturing" || state.kind === "stopping"
      ? Math.max(0, Math.floor((now - state.sinceMs) / 1000))
      : undefined;

  return (
    <Card
      title="Packet capture"
      className="net-card net-capture"
      data-testid="net-capture"
      subtitle={pill}
    >
      {unavailable ? (
        <div className="net-capture-off" data-testid="pcap-off">
          <Dash reason={why(capability)} />
          <span>{why(capability)}</span>
        </div>
      ) : null}
      <div className="net-capture-path">
        <label htmlFor={inputId} className="net-capture-label">
          Save to
        </label>
        <input
          id={inputId}
          className={INPUT_CLASS}
          value={path}
          onChange={(e) => setPath(deviceId, e.target.value)}
          disabled={unavailable || capturing || busy}
          spellCheck={false}
          aria-invalid={error ? true : undefined}
          aria-describedby={error ? errorId : undefined}
        />
        {tauri ? (
          <Button
            variant="ghost"
            onClick={() => void choose()}
            disabled={unavailable || capturing || busy}
          >
            Choose…
          </Button>
        ) : null}
      </div>
      <div className="net-capture-actions">
        {capturing ? (
          <Button
            variant="primary"
            icon="pause"
            onClick={() => void stop()}
            disabled={busy}
            data-testid="pcap-stop"
          >
            Stop capture
          </Button>
        ) : (
          <Button
            variant="primary"
            icon="record"
            onClick={() => void start()}
            disabled={unavailable || busy || path.trim() === ""}
            data-testid="pcap-start"
          >
            Start capture
          </Button>
        )}
        {elapsed !== undefined ? (
          <span className="net-capture-elapsed mono" data-testid="pcap-elapsed">
            {elapsed} s ·{" "}
            {state.kind === "capturing" || state.kind === "stopping" ? state.path : ""}
          </span>
        ) : null}
      </div>
      {error ? (
        <div className="net-capture-error" role="alert" id={errorId} data-testid="pcap-error">
          <span>
            <span className="mono">{error.code}</span> · {error.message}
          </span>
          {error.hint ? <span className="net-capture-hint">{error.hint}</span> : null}
        </div>
      ) : null}
    </Card>
  );
}

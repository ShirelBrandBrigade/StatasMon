import { Button, IconButton, StatusDot } from "@statasmon/ui";
import { useEffect, useRef, useState } from "react";
import { commands } from "../bridge/commands";
import { useDevice, useLogCount } from "../hooks/useDevice";
import { useAppStore } from "../store/app";
import { rememberDetached } from "./detached";
import { formatCount, formatElapsed, sessionStatusLabel, sessionStatusTone } from "./format";

/** Ticks once a second while recording; returns the elapsed recording time in ms. */
function useElapsedMs(deviceId: string): number {
  const session = useDevice(deviceId)?.session;
  const recording = session?.status === "recording";
  const base = session?.elapsed_ms ?? 0;
  // The core reports `elapsed_ms` (paused time excluded) every few seconds;
  // between reports we add the wall-clock time since the last report.
  const anchor = useRef(Date.now());
  const [now, setNow] = useState(() => Date.now());
  // biome-ignore lint/correctness/useExhaustiveDependencies: `session` identity is the resync signal
  useEffect(() => {
    anchor.current = Date.now();
    setNow(anchor.current);
  }, [session]);
  useEffect(() => {
    if (!recording) return undefined;
    const id = setInterval(() => setNow(Date.now()), 1000);
    return () => clearInterval(id);
  }, [recording]);
  return recording ? base + Math.max(0, now - anchor.current) : base;
}

export function RecordingCard({ deviceId }: { deviceId: string }) {
  const entry = useDevice(deviceId);
  const streamKind = useAppStore((s) => s.streamKind);
  const count = useLogCount(deviceId);
  const elapsed = useElapsedMs(deviceId);
  if (!entry) return null;
  const status = entry.session?.status;
  const label = sessionStatusLabel(status);
  const markCount = (entry.session?.marks.length ?? 0) + 1;
  const canUnplug = streamKind === "tauri" && deviceId.startsWith("fake:");

  const primary =
    status === "recording"
      ? { label: "Pause", icon: "pause" as const, run: () => commands.pauseSession(deviceId) }
      : status === "paused"
        ? { label: "Resume", icon: "play" as const, run: () => commands.resumeSession(deviceId) }
        : { label: "Start", icon: "record" as const, run: () => commands.startSession(deviceId) };

  return (
    <section className="rec" data-status={status ?? "idle"} aria-label="Recording">
      <div className="rec-head">
        <b>
          <StatusDot tone={sessionStatusTone(status)} label={label} glow={status === "recording"} />
          {label}
        </b>
        {canUnplug ? (
          <IconButton
            icon="close"
            label="Unplug"
            className="rec-unplug"
            onClick={() => {
              rememberDetached(entry.summary);
              void commands.fakeDetach(deviceId);
            }}
          />
        ) : null}
      </div>
      <span className="mono rec-meta">
        <time>{formatElapsed(elapsed)}</time> · {formatCount(count)} entries
      </span>
      <div className="rec-pb">
        <Button size="sm" icon={primary.icon} onClick={() => void primary.run()}>
          {primary.label}
        </Button>
        <Button
          size="sm"
          icon="mark"
          disabled={status !== "recording" && status !== "paused"}
          onClick={() => void commands.mark(deviceId, `Mark ${markCount}`)}
        >
          Mark
        </Button>
      </div>
    </section>
  );
}

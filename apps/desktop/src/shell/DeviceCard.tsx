import type { DeviceSummary, SessionStatus, Transport } from "@statasmon/protocol";
import { cx, Icon, StatusDot, type Tone } from "@statasmon/ui";
import type { DeviceEntry } from "../store/app";
import { deviceIcon } from "./format";

export interface DeviceCardProps {
  entry: DeviceEntry;
  active: boolean;
  onSelect: (id: string) => void;
}

const TRANSPORT_LABEL: Record<Transport, string> = {
  usb: "USB",
  wifi: "Wi-Fi",
  adb_tcp: "Wi-Fi ADB",
  fake: "Fake",
};

export function transportLabel(t: Transport): string {
  return TRANSPORT_LABEL[t];
}

const SESSION_LABEL: Record<SessionStatus, string> = {
  idle: "idle",
  recording: "recording",
  paused: "paused",
  stopped: "stopped",
};

/** Shown while the device waits for the user to accept the USB debugging prompt. */
export const NEEDS_TRUST_TEXT = "Accept the USB debugging prompt on the device";

/**
 * Second line of the card and the tone it forces on the dot. A connection
 * that is not up wins over the session status; `needs_trust` gets a warn dot
 * because the user has to act on the device.
 */
export function deviceStateLine(
  summary: DeviceSummary,
  session?: SessionStatus,
): { text: string; tone?: Tone; label?: string } {
  const t = transportLabel(summary.transport);
  const st = summary.state;
  switch (st.state) {
    case "needs_trust":
      return { text: NEEDS_TRUST_TEXT, tone: "warn", label: "Needs trust" };
    case "connecting":
      return { text: `${t} · Connecting…` };
    case "error":
      return { text: st.message, tone: "bad", label: "Error" };
    case "detected":
      return { text: `${t} · detected` };
    case "disconnected":
      return { text: `${t} · disconnected` };
    default:
      return { text: `${t} · ${SESSION_LABEL[session ?? "idle"]}` };
  }
}

function dotTone(entry: DeviceEntry): { tone: Tone; label: string } {
  const sev = entry.headline?.severity;
  if (sev === "warning" || sev === "critical") return { tone: "warn", label: "Needs attention" };
  if (entry.session?.status === "recording") return { tone: "ok", label: "Recording" };
  return { tone: "neutral", label: "Idle" };
}

/** One device in the sidebar: icon tile, name, transport · state, status dot. */
export function DeviceCard({ entry, active, onSelect }: DeviceCardProps) {
  const { summary, session } = entry;
  const line = deviceStateLine(summary, session?.status);
  const dot = line.tone ? { tone: line.tone, label: line.label ?? "" } : dotTone(entry);
  return (
    <button
      type="button"
      className={cx("dev", active && "is-on")}
      aria-pressed={active}
      aria-label={`${summary.name} · ${line.text} · ${dot.label}`}
      data-device-id={summary.id}
      data-state={summary.state.state}
      onClick={() => onSelect(summary.id)}
    >
      <span className="dev-ic">
        <Icon name={deviceIcon(summary)} size={16} />
      </span>
      <span className="dev-txt">
        <b>{summary.name}</b>
        <span title={line.text}>{line.text}</span>
      </span>
      <StatusDot tone={dot.tone} label={dot.label} className="dev-st" />
    </button>
  );
}

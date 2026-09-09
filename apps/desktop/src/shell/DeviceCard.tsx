import { cx, Icon, StatusDot, type Tone } from "@statasmon/ui";
import type { DeviceEntry } from "../store/app";
import { deviceIcon, deviceStateLine } from "./format";

export interface DeviceCardProps {
  entry: DeviceEntry;
  active: boolean;
  onSelect: (id: string) => void;
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
  const dot = dotTone(entry);
  const line = deviceStateLine(summary, session?.status);
  return (
    <button
      type="button"
      className={cx("dev", active && "is-on")}
      aria-pressed={active}
      aria-label={`${summary.name} · ${line} · ${dot.label}`}
      data-device-id={summary.id}
      onClick={() => onSelect(summary.id)}
    >
      <span className="dev-ic">
        <Icon name={deviceIcon(summary)} size={16} />
      </span>
      <span className="dev-txt">
        <b>{summary.name}</b>
        <span>{line}</span>
      </span>
      <StatusDot tone={dot.tone} label={dot.label} className="dev-st" />
    </button>
  );
}

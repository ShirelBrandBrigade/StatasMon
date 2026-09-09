import type { ConnectionState, DeviceSummary, SessionStatus, Transport } from "@statasmon/protocol";
import type { IconName, Tone } from "@statasmon/ui";

/** Elapsed recording time as `mm:ss`, or `hh:mm:ss` past one hour. Always tabular mono. */
export function formatElapsed(ms: number): string {
  const total = Math.max(0, Math.floor(ms / 1000));
  const h = Math.floor(total / 3600);
  const m = Math.floor((total % 3600) / 60);
  const s = total % 60;
  const mm = String(m).padStart(2, "0");
  const ss = String(s).padStart(2, "0");
  return h > 0 ? `${h}:${mm}:${ss}` : `${mm}:${ss}`;
}

/** Integer with thin-space thousands grouping, matching `formatValue` in the protocol package. */
export function formatCount(n: number): string {
  return Math.round(n).toLocaleString("en-US").replace(/,/g, " ");
}

const TRANSPORT_LABEL: Record<Transport, string> = {
  usb: "USB",
  wifi: "Wi-Fi",
  adb_tcp: "ADB",
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

const CONNECTION_LABEL: Record<ConnectionState["state"], string | undefined> = {
  detected: "detected",
  needs_trust: "needs trust",
  connecting: "connecting",
  tier1: undefined,
  tier2: undefined,
  disconnected: "disconnected",
  error: "error",
};

/**
 * The second line of a DeviceCard: transport · state. A connection that is
 * not up wins over the session status ("USB · needs trust").
 */
export function deviceStateLine(summary: DeviceSummary, session?: SessionStatus): string {
  const conn = CONNECTION_LABEL[summary.state.state];
  const state = conn ?? SESSION_LABEL[session ?? "idle"];
  return `${transportLabel(summary.transport)} · ${state}`;
}

/** Sidebar icon for a device: phone for iPhones, tablet for other iOS devices, android for Android. */
export function deviceIcon(summary: DeviceSummary): IconName {
  if (summary.platform === "android") return "android";
  if (summary.platform === "ios") return /iphone/i.test(summary.model_name) ? "phone" : "tablet";
  return "device";
}

export function sessionStatusLabel(status: SessionStatus | undefined): string {
  switch (status) {
    case "recording":
      return "Recording";
    case "paused":
      return "Paused";
    case "stopped":
      return "Stopped";
    default:
      return "Idle";
  }
}

export function sessionStatusTone(status: SessionStatus | undefined): Tone {
  switch (status) {
    case "recording":
      return "bad";
    case "paused":
      return "warn";
    default:
      return "neutral";
  }
}

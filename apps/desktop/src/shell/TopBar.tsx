import type { Headline, Severity, Tone } from "@statasmon/protocol";
import { Button, Pill, Segment } from "@statasmon/ui";
import { openExportDialog } from "../export/exportStore";
import { useDevice } from "../hooks/useDevice";
import { type Range, useAppStore } from "../store/app";
import { transportLabel } from "./format";

export const RANGE_OPTIONS = [
  { value: "live", label: "Live" },
  { value: "5m", label: "5 min" },
  { value: "15m", label: "15 min" },
  { value: "1h", label: "1 h" },
  { value: "session", label: "Session" },
] as const satisfies ReadonlyArray<{ value: Range; label: string }>;

/** Two or three plain words per headline rule; the full sentence and cause live in the pill's tooltip. */
const RULE_LABEL: Record<string, string> = {
  thermal_critical: "Critically hot",
  thermal_serious: "Running hot",
  thermal_fair: "Warm",
  app_crashed: "App crashed",
  app_anr: "App not responding",
  memory_warning: "Memory warning",
  memory_kill: "App closed for memory",
  memory_pressure: "Low memory",
  battery_hot: "Battery hot",
  network_struggling: "Network struggling",
  developer_mode_off: "Limited mode",
};

const SEVERITY_TONE: Record<Severity, Tone> = {
  nominal: "ok",
  notice: "info",
  warning: "warn",
  critical: "bad",
};

/** The headline as one short pill (none while everything is normal). */
export function headlinePill(headline: Headline): { label: string; tone: Tone } | undefined {
  if (headline.severity === "nominal") return undefined;
  const label = RULE_LABEL[headline.rule] ?? headline.rule.replace(/_/g, " ");
  return { label, tone: SEVERITY_TONE[headline.severity] };
}

/**
 * Device name, a fixed one-line subtitle (model · link) and the status pills
 * on the left; range segment and Export on the right. Nothing here changes
 * height while the session runs: the headline is a pill, not a sentence.
 */
export function TopBar({ deviceId }: { deviceId: string }) {
  const entry = useDevice(deviceId);
  const range = useAppStore((s) => s.range);
  const setRange = useAppStore((s) => s.setRange);
  const headline = entry?.headline;
  const summary = entry?.summary;
  const name = summary?.name || summary?.model_name || "Device";
  const status = headline ? headlinePill(headline) : undefined;

  return (
    <header className="hd">
      <div className="hd-text">
        <h1 title={name}>{name}</h1>
        <p className="hd-sub">
          {summary
            ? `${summary.model_name} · ${transportLabel(summary.transport)}`
            : "Waiting for the device…"}
        </p>
        <ul className="hd-pills" aria-label="Status">
          {status ? (
            <li key="headline" title={`${headline?.sentence}. ${headline?.cause}`}>
              <Pill tone={status.tone} data-testid="headline-pill">
                {status.label}
              </Pill>
            </li>
          ) : null}
          {headline?.pills.map((p) => (
            <li key={p.label}>
              <Pill tone={p.tone}>{p.label}</Pill>
            </li>
          ))}
        </ul>
      </div>
      <div className="hd-tools">
        <Segment name="Range" options={RANGE_OPTIONS} value={range} onChange={setRange} />
        <Button
          variant="primary"
          icon="download"
          aria-haspopup="dialog"
          onClick={() => openExportDialog({ deviceId })}
        >
          Export
        </Button>
      </div>
    </header>
  );
}

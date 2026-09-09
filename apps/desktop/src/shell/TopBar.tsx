import { Button, Pill, Segment } from "@statasmon/ui";
import { openExportDialog } from "../export/exportStore";
import { useDevice } from "../hooks/useDevice";
import { type Range, useAppStore } from "../store/app";

export const RANGE_OPTIONS = [
  { value: "live", label: "Live" },
  { value: "5m", label: "5 min" },
  { value: "15m", label: "15 min" },
  { value: "1h", label: "1 h" },
  { value: "session", label: "Session" },
] as const satisfies ReadonlyArray<{ value: Range; label: string }>;

/** Headline sentence + cause + status pills on the left; range segment and Export on the right. */
export function TopBar({ deviceId }: { deviceId: string }) {
  const entry = useDevice(deviceId);
  const range = useAppStore((s) => s.range);
  const setRange = useAppStore((s) => s.setRange);
  const headline = entry?.headline;
  const model = entry?.summary.model_name ?? "device";

  return (
    <header className="hd">
      <div className="hd-text">
        <h1>{headline?.sentence ?? `Waiting for ${model}…`}</h1>
        {headline?.cause ? <p>{headline.cause}</p> : null}
        {headline && headline.pills.length > 0 ? (
          <ul className="hd-pills" aria-label="Status">
            {headline.pills.map((p) => (
              <li key={p.label}>
                <Pill tone={p.tone}>{p.label}</Pill>
              </li>
            ))}
          </ul>
        ) : null}
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

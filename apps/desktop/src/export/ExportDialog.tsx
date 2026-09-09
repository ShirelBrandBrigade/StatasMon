import type { ExportFormat, ExportResult } from "@statasmon/protocol";
import { Button, Segment } from "@statasmon/ui";
import { useEffect, useId, useMemo, useRef, useState } from "react";
import { commands } from "../bridge/commands";
import { isTauri } from "../bridge/stream";
import { useProcesses, useRangeMs } from "../hooks/useDevice";
import { useAppStore } from "../store/app";
import { DEFAULT_LOG_FILTER, useLogFilterStore } from "../store/logFilter";
import { DialogFrame } from "./DialogFrame";
import {
  appChoices,
  buildRequest,
  buildScope,
  defaultExportPath,
  describeExportError,
  type ExportFailure,
  FORMAT_OPTIONS,
  pickExportProgress,
  resultLine,
  SCOPE_ORDER,
  type ScopeKind,
  toProtocolFilter,
  validatePath,
  withExtension,
} from "./exportModel";
import { listenExportEvents, useExportStore } from "./exportStore";

const INPUT_CLASS =
  "mono box-border h-8 min-h-8 w-full rounded-ctl border border-edge bg-card px-2.5 text-body text-fg outline-none placeholder:text-dim focus-visible:outline-2 focus-visible:outline-cyan focus-visible:-outline-offset-1 aria-[invalid=true]:border-red";

const SELECT_CLASS =
  "box-border h-8 min-h-8 min-w-0 flex-1 rounded-ctl border border-edge bg-card px-2 text-body text-fg outline-none focus-visible:outline-2 focus-visible:outline-cyan focus-visible:-outline-offset-1 disabled:opacity-50";

/** Opens the native save sheet inside Tauri; resolves to undefined when cancelled or in a browser. */
async function chooseSavePath(defaultPath: string): Promise<string | undefined> {
  if (!isTauri()) return undefined;
  const { save } = await import("@tauri-apps/plugin-dialog");
  const picked = await save({ defaultPath });
  return picked ?? undefined;
}

async function reveal(path: string): Promise<void> {
  if (!isTauri()) return;
  const { revealItemInDir } = await import("@tauri-apps/plugin-opener");
  await revealItemInDir(path);
}

type Phase =
  | { kind: "idle" }
  | { kind: "running"; startedAt: number }
  | { kind: "done"; result: ExportResult }
  | { kind: "failed"; failure: ExportFailure };

/** Mounted once (App); renders nothing until `openExportDialog` / ⌘E / the log card opens it. */
export function ExportDialog() {
  const open = useExportStore((s) => s.open);
  useEffect(() => listenExportEvents(), []);
  return open ? <ExportDialogBody /> : null;
}

function ExportDialogBody() {
  const uid = useId();
  const close = useExportStore((s) => s.close);
  const preset = useExportStore((s) => s.preset);
  const activeDeviceId = useAppStore((s) => s.activeDeviceId);
  const deviceId = preset.deviceId ?? activeDeviceId;
  const entry = useAppStore((s) => (deviceId ? s.devices[deviceId] : undefined));
  const deviceName = preset.deviceName ?? entry?.summary.name ?? "device";
  const sessionId = preset.sessionId ?? null;
  const rangeMs = useRangeMs();
  const rangeLabel = useAppStore((s) => s.range);
  const logFilter = useLogFilterStore((s) =>
    deviceId ? (s.byDevice[deviceId] ?? DEFAULT_LOG_FILTER) : DEFAULT_LOG_FILTER,
  );
  const processes = useProcesses(deviceId);
  const apps = useMemo(() => appChoices(processes), [processes]);
  const progressMap = useAppStore((s) => s.progress);
  const tauri = isTauri();

  const [format, setFormat] = useState<ExportFormat>(preset.format ?? "log");
  const [scope, setScope] = useState<ScopeKind>(preset.scope ?? "session");
  const [appId, setAppId] = useState("");
  const [path, setPath] = useState(() =>
    defaultExportPath(deviceName, preset.format ?? "log", new Date()),
  );
  const [pathDirty, setPathDirty] = useState(false);
  const [error, setError] = useState<string>();
  const [phase, setPhase] = useState<Phase>({ kind: "idle" });
  const pathRef = useRef<HTMLInputElement>(null);
  const seqs = preset.seqs ?? [];

  const changeFormat = (f: ExportFormat) => {
    setFormat(f);
    if (!pathDirty) setPath((p) => withExtension(p, f));
  };

  const progress =
    phase.kind === "running" ? pickExportProgress(progressMap, phase.startedAt) : undefined;
  // The bar stays full next to the result line once the export is done.
  const pct = phase.kind === "done" ? 100 : (progress?.pct ?? 0);
  const progressText = phase.kind === "done" ? "done" : (progress?.message ?? "starting…");

  const run = async () => {
    if (!deviceId) {
      setError("No device selected.");
      return;
    }
    const pathError = validatePath(path);
    if (pathError) {
      setError(pathError);
      pathRef.current?.focus();
      return;
    }
    const built = buildScope({
      kind: scope,
      rangeMs,
      nowMs: entry?.lastSampleMs ?? Date.now(),
      filter: toProtocolFilter(deviceId, sessionId, logFilter),
      appId,
      seqs,
    });
    if ("error" in built) {
      setError(built.error);
      return;
    }
    setError(undefined);
    const startedAt = Date.now();
    setPhase({ kind: "running", startedAt });
    try {
      const result = await commands.exportData(
        buildRequest({ deviceId, sessionId, format, scope: built.scope, path }),
      );
      if (!result || typeof result !== "object") {
        setPhase({
          kind: "failed",
          failure: { code: "internal", message: "The core returned no result." },
        });
        return;
      }
      setPhase({ kind: "done", result });
    } catch (e) {
      setPhase({ kind: "failed", failure: describeExportError(e) });
    }
  };

  const titleId = `${uid}-title`;
  const pathId = `${uid}-path`;
  const appSelectId = `${uid}-app`;
  const errId = `${uid}-err`;
  const running = phase.kind === "running";

  const scopeLabel = (k: ScopeKind): string => {
    switch (k) {
      case "session":
        return "Whole session";
      case "range":
        return rangeMs === undefined
          ? "Current range (whole session)"
          : `Current range (${rangeLabel})`;
      case "filter":
        return "Current log filter";
      case "app":
        return "One app";
      case "selection":
        return `Selected log lines (${seqs.length})`;
    }
  };

  return (
    <DialogFrame titleId={titleId} title="Export" onClose={close} testId="export-scrim">
      <p className="m-0 text-body text-mut">
        {sessionId ? (
          <>
            Stored session of <b className="text-fg">{deviceName}</b>
          </>
        ) : (
          <>
            Live session of <b className="text-fg">{deviceName}</b>
          </>
        )}
      </p>

      <div className="flex flex-col gap-1.5">
        <span className="section-label">Format</span>
        <Segment name="Format" options={FORMAT_OPTIONS} value={format} onChange={changeFormat} />
      </div>

      <fieldset className="m-0 flex flex-col gap-1.5 border-0 p-0" aria-label="Scope">
        <legend className="section-label mb-1.5 p-0">Scope</legend>
        {SCOPE_ORDER.map((k) => {
          const disabled = k === "selection" && seqs.length === 0;
          const radioId = `${uid}-scope-${k}`;
          return (
            <div key={k} className="flex min-h-8 items-center gap-2.5">
              <input
                id={radioId}
                type="radio"
                name={`${uid}-scope`}
                value={k}
                checked={scope === k}
                disabled={disabled}
                onChange={() => setScope(k)}
                className="m-0 h-4 w-4 accent-cyan"
              />
              <label htmlFor={radioId} className={`text-body ${disabled ? "text-dim" : "text-fg"}`}>
                {scopeLabel(k)}
              </label>
              {k === "app" ? (
                <select
                  id={appSelectId}
                  aria-label="App"
                  className={SELECT_CLASS}
                  value={appId}
                  disabled={scope !== "app"}
                  onChange={(e) => setAppId(e.target.value)}
                >
                  <option value="">
                    {apps.length === 0 ? "No processes yet" : "Choose an app…"}
                  </option>
                  {apps.map((a) => (
                    <option key={a.id} value={a.id}>
                      {a.label}
                    </option>
                  ))}
                </select>
              ) : null}
            </div>
          );
        })}
      </fieldset>

      <div className="flex flex-col gap-1 text-subtitle text-mut">
        <label htmlFor={pathId}>Destination</label>
        <div className="flex gap-2">
          <input
            id={pathId}
            ref={pathRef}
            className={INPUT_CLASS}
            value={path}
            spellCheck={false}
            autoComplete="off"
            aria-invalid={error && validatePath(path) ? true : undefined}
            onChange={(e) => {
              setPath(e.target.value);
              setPathDirty(true);
            }}
          />
          <Button
            size="sm"
            disabled={!tauri}
            title={tauri ? undefined : "Available in the desktop app"}
            onClick={() => {
              void chooseSavePath(path).then((picked) => {
                if (picked) {
                  setPath(picked);
                  setPathDirty(true);
                }
              });
            }}
          >
            Choose…
          </Button>
        </div>
      </div>

      {error ? (
        <p id={errId} role="alert" className="m-0 text-body text-red">
          {error}
        </p>
      ) : null}

      <div className="flex items-center gap-3">
        <Button
          variant="primary"
          icon="download"
          disabled={running}
          aria-describedby={error ? errId : undefined}
          onClick={() => void run()}
        >
          {running ? "Exporting…" : "Export"}
        </Button>
        {phase.kind === "running" || phase.kind === "done" ? (
          <div className="flex min-w-0 flex-1 flex-col gap-1" data-testid="export-progress">
            <progress className="h-1.5 w-full" max={100} value={pct} aria-label="Export progress" />
            <span className="mono text-axis text-mut">
              {Math.round(pct)} % · {progressText}
            </span>
          </div>
        ) : null}
      </div>

      {phase.kind === "done" ? (
        <div className="flex items-center gap-3" role="status" data-testid="export-result">
          <span
            className="mono min-w-0 flex-1 truncate text-body text-mint"
            title={phase.result.path}
          >
            {resultLine(phase.result)}
          </span>
          <Button
            size="sm"
            disabled={!tauri}
            title={tauri ? undefined : "Available in the desktop app"}
            onClick={() => void reveal(phase.result.path)}
          >
            Reveal
          </Button>
        </div>
      ) : null}
      {phase.kind === "failed" ? (
        <p
          role="status"
          data-outcome="fail"
          className="m-0 flex flex-col gap-0.5 text-body text-red"
        >
          <span>
            <span className="mono">{phase.failure.code}</span> · {phase.failure.message}
          </span>
          {phase.failure.hint ? <span className="text-mut">{phase.failure.hint}</span> : null}
        </p>
      ) : null}
    </DialogFrame>
  );
}

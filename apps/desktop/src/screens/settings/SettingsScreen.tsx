import "./settings.css";
import { formatValue, type Settings } from "@statasmon/protocol";
import { Button, Card, Dash, KeyValue } from "@statasmon/ui";
import { type ReactNode, useCallback, useEffect, useId, useState } from "react";
import pkg from "../../../package.json";
import { commands, type StoreStats } from "../../bridge/commands";
import { isTauri } from "../../bridge/stream";
import { formatCount } from "../../shell/format";
import {
  type Draft,
  type DraftErrors,
  type DraftField,
  defaultSettings,
  fromDraft,
  isDirty,
  LIMITS,
  toDraft,
  validateDraft,
} from "./settingsModel";

/** How long the "Saved" line stays. */
export const SAVED_MS = 2000;

const PLATFORM_TOOLS_URL = "https://developer.android.com/tools/releases/platform-tools";

function describeError(e: unknown): string {
  if (e && typeof e === "object" && typeof (e as { message?: unknown }).message === "string") {
    return (e as { message: string }).message;
  }
  return typeof e === "string" ? e : "Something went wrong";
}

/** Native file picker (Tauri only); the browser build keeps the text field. */
async function pickAdb(): Promise<string | undefined> {
  if (!isTauri()) return undefined;
  const { open } = await import("@tauri-apps/plugin-dialog");
  const picked = await open({ multiple: false, directory: false, title: "Choose adb" });
  return typeof picked === "string" ? picked : undefined;
}

async function reveal(path: string): Promise<void> {
  if (!isTauri()) return;
  const { revealItemInDir } = await import("@tauri-apps/plugin-opener");
  await revealItemInDir(path);
}

interface NumberFieldProps {
  field: DraftField;
  label: string;
  unit: string;
  hint: string;
  value: string;
  error?: string;
  onChange: (field: DraftField, value: string) => void;
}

function NumberField({ field, label, unit, hint, value, error, onChange }: NumberFieldProps) {
  const id = useId();
  const hintId = `${id}-hint`;
  return (
    <div className="st-field" data-field={field}>
      <label htmlFor={id}>{label}</label>
      <span className="st-field-control">
        <input
          id={id}
          type="text"
          inputMode="numeric"
          className="st-input is-num mono"
          value={value}
          aria-invalid={error ? "true" : undefined}
          aria-describedby={hintId}
          onChange={(e) => onChange(field, e.target.value)}
        />
        <span>{unit}</span>
      </span>
      <span id={hintId} className={error ? "st-field-hint is-error" : "st-field-hint"}>
        {error ?? hint}
      </span>
    </div>
  );
}

/**
 * Settings (docs/PLAN.md 6.6): sampling, retention, Android adb, storage,
 * updates and about. Loads from the core, saves through `set_settings`
 * (the core validates again and answers with what it stored). Sampling,
 * retention and the adb path are read at startup, so they take effect at
 * the next launch; the screen says so rather than pretending otherwise.
 */
export function SettingsScreen() {
  const [saved, setSaved] = useState<Settings>();
  const [draft, setDraft] = useState<Draft>(() => toDraft(defaultSettings()));
  const [stats, setStats] = useState<StoreStats | null>(null);
  const [loadError, setLoadError] = useState<string>();
  const [saving, setSaving] = useState(false);
  const [status, setStatus] = useState<{ text: string; tone: "ok" | "bad" }>();
  const tauri = isTauri();

  useEffect(() => {
    let cancelled = false;
    void commands
      .getSettings()
      .then((s) => {
        if (cancelled) return;
        const settings = s ?? defaultSettings();
        setSaved(settings);
        setDraft(toDraft(settings));
      })
      .catch((e) => {
        if (!cancelled) setLoadError(describeError(e));
      });
    void commands
      .storeStats()
      .then((st) => {
        if (!cancelled) setStats(st ?? null);
      })
      .catch(() => {
        if (!cancelled) setStats(null);
      });
    return () => {
      cancelled = true;
    };
  }, []);

  useEffect(() => {
    if (status?.tone !== "ok") return undefined;
    const t = setTimeout(() => setStatus(undefined), SAVED_MS);
    return () => clearTimeout(t);
  }, [status]);

  const errors: DraftErrors = validateDraft(draft);
  const hasErrors = Object.keys(errors).length > 0;
  const dirty = saved ? isDirty(draft, saved) : false;

  const change = useCallback((field: DraftField, value: string) => {
    setDraft((d) => ({ ...d, [field]: value }));
  }, []);

  const save = async () => {
    if (!saved) return;
    if (hasErrors) {
      setStatus({ text: "Fix the highlighted fields first", tone: "bad" });
      return;
    }
    setSaving(true);
    try {
      const stored = await commands.setSettings(fromDraft(draft, saved));
      const next = stored ?? fromDraft(draft, saved);
      setSaved(next);
      setDraft(toDraft(next));
      setStatus({ text: "Saved", tone: "ok" });
    } catch (e) {
      setStatus({ text: describeError(e), tone: "bad" });
    } finally {
      setSaving(false);
    }
  };

  const revert = () => {
    if (saved) setDraft(toDraft(saved));
    setStatus(undefined);
  };

  const choose = async () => {
    try {
      const path = await pickAdb();
      if (path) change("adb_path", path);
    } catch (e) {
      setStatus({ text: describeError(e), tone: "bad" });
    }
  };

  const storageRows: Array<{ label: string; value: ReactNode }> = [
    {
      label: "Folder",
      value: stats ? stats.root : <Dash reason="Session storage is not available in this run" />,
    },
    {
      label: "Total size",
      value: stats ? (
        formatValue({ unit: "bytes" }, stats.total_bytes)
      ) : (
        <Dash reason="Session storage is not available in this run" />
      ),
    },
    {
      label: "Sessions",
      value: stats ? (
        formatCount(stats.session_count)
      ) : (
        <Dash reason="Session storage is not available in this run" />
      ),
    },
  ];

  return (
    <div className="st" data-testid="settings-screen" data-dirty={dirty} data-loaded={!!saved}>
      <header className="st-head">
        <div>
          <h1>Settings</h1>
          <p>
            Stored in <code className="mono">settings.json</code> in the app-data folder. Nothing
            here contacts the network.
          </p>
        </div>
        <div className="st-head-tools">
          {status ? (
            <span role="status" className={status.tone === "bad" ? "st-saved is-bad" : "st-saved"}>
              {status.text}
            </span>
          ) : null}
          <Button onClick={revert} disabled={!dirty || saving}>
            Revert
          </Button>
          <Button
            variant="primary"
            icon="check"
            onClick={() => void save()}
            disabled={!saved || !dirty || saving}
            aria-busy={saving}
          >
            {saving ? "Saving…" : "Save"}
          </Button>
        </div>
      </header>

      {loadError ? (
        <p role="alert" className="m-0 text-body text-red">
          Settings could not be loaded: {loadError}
        </p>
      ) : null}

      <div className="st-grid">
        <Card title="Sampling" subtitle="applies at the next launch" aria-label="Sampling">
          <p className="st-note">
            Cadence handed to devices when they connect. Saved values are read when the app starts;
            devices already connected keep their cadence until the app is relaunched.
          </p>
          <div className="st-fields">
            <NumberField
              field="fast_ms"
              label="Fast"
              unit="ms"
              hint={`CPU, memory, network. At least ${formatCount(LIMITS.fast_ms.min)} ms.`}
              value={draft.fast_ms}
              error={errors.fast_ms}
              onChange={change}
            />
            <NumberField
              field="medium_ms"
              label="Medium"
              unit="ms"
              hint={`Battery, processes, Wi-Fi. At least ${formatCount(LIMITS.medium_ms.min)} ms.`}
              value={draft.medium_ms}
              error={errors.medium_ms}
              onChange={change}
            />
            <NumberField
              field="slow_ms"
              label="Slow"
              unit="ms"
              hint={`Thermal, storage, network totals. At least ${formatCount(LIMITS.slow_ms.min)} ms.`}
              value={draft.slow_ms}
              error={errors.slow_ms}
              onChange={change}
            />
            <NumberField
              field="heavy_ms"
              label="Heavy"
              unit="ms"
              hint={`Expensive snapshots (dumpsys meminfo). 0 turns them off; otherwise at least ${formatCount(LIMITS.heavy_ms.min)} ms.`}
              value={draft.heavy_ms}
              error={errors.heavy_ms}
              onChange={change}
            />
          </div>
        </Card>

        <Card title="Retention" subtitle="applies at the next launch" aria-label="Retention">
          <p className="st-note">
            A recording is split into a new session past the hour cap; ended sessions older than the
            day limit are deleted by a daily sweep, then the oldest past the size cap.
          </p>
          <div className="st-fields">
            <NumberField
              field="max_session_hours"
              label="Hours per session"
              unit="h"
              hint={`From ${LIMITS.max_session_hours.min} to ${LIMITS.max_session_hours.max}.`}
              value={draft.max_session_hours}
              error={errors.max_session_hours}
              onChange={change}
            />
            <NumberField
              field="keep_days"
              label="Days to keep"
              unit="days"
              hint={`From ${LIMITS.keep_days.min} to ${LIMITS.keep_days.max}.`}
              value={draft.keep_days}
              error={errors.keep_days}
              onChange={change}
            />
            <NumberField
              field="max_total_gb"
              label="Total size cap"
              unit="GB"
              hint="Empty means no cap."
              value={draft.max_total_gb}
              error={errors.max_total_gb}
              onChange={change}
            />
          </div>
        </Card>

        <Card
          title="Android"
          subtitle={draft.adb_path.trim() ? "explicit path" : "auto-detected"}
          aria-label="Android"
        >
          <p className="st-note">
            Leave empty to search the usual places (an app-managed copy, PATH, ~/Downloads, the
            Android SDK). An explicit path is checked when saved and used at the next launch.{" "}
            <a href={PLATFORM_TOOLS_URL} target="_blank" rel="noreferrer">
              Download platform-tools
            </a>
          </p>
          <div className="st-path">
            <input
              type="text"
              className="st-input is-text mono"
              aria-label="adb path"
              placeholder="Auto-detected"
              value={draft.adb_path}
              onChange={(e) => change("adb_path", e.target.value)}
              spellCheck={false}
            />
            {tauri ? (
              <Button size="sm" onClick={() => void choose()}>
                Choose…
              </Button>
            ) : null}
          </div>
        </Card>

        <Card title="Storage" aria-label="Storage" className="st-storage">
          <KeyValue rows={storageRows} />
          {tauri && stats ? (
            <div className="st-actions">
              <Button size="sm" onClick={() => void reveal(stats.root)}>
                Reveal
              </Button>
            </div>
          ) : null}
        </Card>

        <Card title="Updates" aria-label="Updates">
          <label className="st-check">
            <input
              type="checkbox"
              checked={draft.update_check}
              onChange={(e) => setDraft((d) => ({ ...d, update_check: e.target.checked }))}
            />
            Check for updates on launch (opt-in)
          </label>
          <p className="st-note">
            When enabled, StatasMon checks once at launch and shows a notice if a newer version exists. Nothing is installed without your action.</p>
        </Card>

        <Card title="About" aria-label="About" className="st-about">
          <KeyValue
            rows={[
              { label: "App", value: "StatasMon" },
              { label: "Version", value: pkg.version },
              { label: "Theme", value: "Dark (the only one for now)" },
            ]}
          />
          <p className="st-note">
            Documentation lives in the repository: docs/FEATURES.md (every field and metric),
            docs/DESIGN-SYSTEM.md, docs/research (what each platform can report).
          </p>
        </Card>
      </div>
    </div>
  );
}

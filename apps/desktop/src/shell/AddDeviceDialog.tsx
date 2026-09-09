import type { DeviceAction } from "@statasmon/protocol";
import { Button, cx, IconButton } from "@statasmon/ui";
import {
  type FormEvent,
  type KeyboardEvent as ReactKeyboardEvent,
  type RefObject,
  useEffect,
  useId,
  useRef,
  useState,
} from "react";
import { createPortal } from "react-dom";
import { commands } from "../bridge/commands";

/**
 * `adb pair` / `adb connect` are not tied to a device that exists yet: the
 * core routes them through the Android discovery, which then attaches the
 * new device on success. The command still needs a device id, so this
 * placeholder is passed and recognised by the core.
 */
export const PENDING_ANDROID_ID = "android:pending";

export const DEFAULT_CONNECT_PORT = 5555;

export type OutcomeKind = "ok" | "fail" | "none";

export interface Outcome {
  kind: OutcomeKind;
  text: string;
  hint?: string;
}

/** Neutral line shown when the core answers with nothing (browser replay, older core). */
export const NO_RESPONSE_TEXT = "No response";

function readString(o: Record<string, unknown>, ...keys: string[]): string | undefined {
  for (const k of keys) {
    const v = o[k];
    if (typeof v === "string" && v.trim().length > 0) return v;
  }
  return undefined;
}

/** Turns whatever `device_action` resolved to into one readable line. */
export function describeOutcome(result: unknown, verb: "pair" | "connect"): Outcome {
  if (result === null || result === undefined) return { kind: "none", text: NO_RESPONSE_TEXT };
  if (typeof result === "string") return { kind: "ok", text: result };
  if (typeof result === "object") {
    const o = result as Record<string, unknown>;
    const hint = readString(o, "hint");
    const text = readString(o, "message", "output", "text", "detail");
    if (o.ok === false || typeof o.error === "string") {
      return { kind: "fail", text: text ?? readString(o, "error") ?? "Failed", hint };
    }
    switch (o.outcome) {
      case "done":
        return {
          kind: "ok",
          text:
            text ??
            (verb === "pair"
              ? "Paired. Now connect to the device below."
              : "Connected. The device will appear in the list."),
          hint,
        };
      case "started":
        return { kind: "ok", text: text ?? `${verb === "pair" ? "Pairing" : "Connecting"}…`, hint };
      case "file":
        return { kind: "ok", text: text ?? String(o.path ?? "Done"), hint };
      default:
        if (text) return { kind: "ok", text, hint };
    }
    return { kind: "ok", text: JSON.stringify(result) };
  }
  return { kind: "ok", text: String(result) };
}

/** Turns a rejected `device_action` (`{ code, message, hint }` or anything else) into a failure line. */
export function describeFailure(e: unknown): Outcome {
  if (e && typeof e === "object") {
    const o = e as Record<string, unknown>;
    const text = readString(o, "message") ?? (e instanceof Error ? e.message : undefined);
    return { kind: "fail", text: text ?? "Failed", hint: readString(o, "hint") };
  }
  return { kind: "fail", text: typeof e === "string" && e ? e : "Failed" };
}

export function validateHost(host: string): string | undefined {
  return host.trim().length === 0 ? "Enter the device's IP address or host name." : undefined;
}

export function validatePort(port: string): string | undefined {
  const n = Number(port.trim());
  if (!/^\d+$/.test(port.trim()) || !Number.isInteger(n) || n < 1 || n > 65535) {
    return "Port must be a number from 1 to 65535.";
  }
  return undefined;
}

export function validateCode(code: string): string | undefined {
  return /^\d{6}$/.test(code.trim()) ? undefined : "The pairing code is 6 digits.";
}

const FOCUSABLE =
  'a[href], button:not([disabled]), input:not([disabled]), select:not([disabled]), textarea:not([disabled]), [tabindex]:not([tabindex="-1"])';

const INPUT_CLASS =
  "mono box-border h-8 min-h-8 w-full rounded-ctl border border-edge bg-card px-2.5 text-body text-fg outline-none placeholder:text-dim focus-visible:outline-2 focus-visible:outline-cyan focus-visible:-outline-offset-1 aria-[invalid=true]:border-red";

function Field({
  id,
  label,
  value,
  onChange,
  placeholder,
  error,
  inputMode = "text",
  maxLength,
  inputRef,
}: {
  id: string;
  label: string;
  value: string;
  onChange: (v: string) => void;
  placeholder?: string;
  error?: string;
  inputMode?: "text" | "numeric" | "decimal";
  maxLength?: number;
  inputRef?: RefObject<HTMLInputElement | null>;
}) {
  const errId = `${id}-err`;
  return (
    <div className="flex min-w-0 flex-1 flex-col gap-1 text-subtitle text-mut">
      <label htmlFor={id}>{label}</label>
      <input
        id={id}
        ref={inputRef}
        className={INPUT_CLASS}
        value={value}
        placeholder={placeholder}
        inputMode={inputMode}
        maxLength={maxLength}
        autoComplete="off"
        spellCheck={false}
        aria-invalid={error ? true : undefined}
        aria-describedby={error ? errId : undefined}
        onChange={(e) => onChange(e.target.value)}
      />
      {error ? (
        <span id={errId} className="text-red">
          {error}
        </span>
      ) : null}
    </div>
  );
}

function OutcomeLine({ outcome, id }: { outcome: Outcome | undefined; id: string }) {
  if (!outcome) return null;
  const color =
    outcome.kind === "ok" ? "text-mint" : outcome.kind === "fail" ? "text-red" : "text-mut";
  return (
    <p
      id={id}
      role="status"
      data-outcome={outcome.kind}
      className={cx("m-0 flex flex-col gap-0.5 text-body", color)}
    >
      <span>{outcome.text}</span>
      {outcome.hint ? <span className="text-mut">{outcome.hint}</span> : null}
    </p>
  );
}

export interface AddDeviceDialogProps {
  open: boolean;
  onClose: () => void;
}

/**
 * "Add a device": wireless ADB pairing and connecting (Phase 2.7), plus the
 * USB instructions. Glass modal, focus trapped, Escape closes, the dialog
 * stays open on failure so the user can correct the fields.
 */
export function AddDeviceDialog({ open, onClose }: AddDeviceDialogProps) {
  const uid = useId();
  const dialogRef = useRef<HTMLDivElement>(null);
  const firstFieldRef = useRef<HTMLInputElement>(null);

  const [pairHost, setPairHost] = useState("");
  const [pairPort, setPairPort] = useState("");
  const [pairCode, setPairCode] = useState("");
  const [pairErrors, setPairErrors] = useState<{ host?: string; port?: string; code?: string }>({});
  const [pairBusy, setPairBusy] = useState(false);
  const [pairOutcome, setPairOutcome] = useState<Outcome>();

  const [connHost, setConnHost] = useState("");
  const [connPort, setConnPort] = useState(String(DEFAULT_CONNECT_PORT));
  const [connErrors, setConnErrors] = useState<{ host?: string; port?: string }>({});
  const [connBusy, setConnBusy] = useState(false);
  const [connOutcome, setConnOutcome] = useState<Outcome>();

  const [devOptionsOpen, setDevOptionsOpen] = useState(false);

  // Focus the first field on open and give focus back to the opener on close.
  useEffect(() => {
    if (!open) return;
    const opener = document.activeElement as HTMLElement | null;
    firstFieldRef.current?.focus();
    return () => {
      opener?.focus?.();
    };
  }, [open]);

  if (!open) return null;

  const onKeyDown = (e: ReactKeyboardEvent<HTMLDivElement>) => {
    if (e.key === "Escape") {
      e.preventDefault();
      e.stopPropagation();
      onClose();
      return;
    }
    if (e.key !== "Tab" || !dialogRef.current) return;
    const items = Array.from(dialogRef.current.querySelectorAll<HTMLElement>(FOCUSABLE));
    if (items.length === 0) return;
    const first = items[0];
    const last = items[items.length - 1];
    const active = document.activeElement;
    if (e.shiftKey && (active === first || !dialogRef.current.contains(active))) {
      e.preventDefault();
      last.focus();
    } else if (!e.shiftKey && active === last) {
      e.preventDefault();
      first.focus();
    }
  };

  const run = async (
    action: DeviceAction,
    verb: "pair" | "connect",
    setBusy: (b: boolean) => void,
    setOutcome: (o: Outcome) => void,
  ) => {
    setBusy(true);
    try {
      const result = await commands.deviceAction(PENDING_ANDROID_ID, action);
      setOutcome(describeOutcome(result, verb));
      return true;
    } catch (e) {
      setOutcome(describeFailure(e));
      return false;
    } finally {
      setBusy(false);
    }
  };

  const submitPair = (e: FormEvent) => {
    e.preventDefault();
    const errors = {
      host: validateHost(pairHost),
      port: validatePort(pairPort),
      code: validateCode(pairCode),
    };
    setPairErrors(errors);
    if (errors.host || errors.port || errors.code) return;
    const host = pairHost.trim();
    void run(
      { action: "adb_pair", host, port: Number(pairPort.trim()), code: pairCode.trim() },
      "pair",
      setPairBusy,
      setPairOutcome,
    ).then((ok) => {
      if (ok && connHost.trim().length === 0) setConnHost(host);
    });
  };

  const submitConnect = (e: FormEvent) => {
    e.preventDefault();
    const errors = { host: validateHost(connHost), port: validatePort(connPort) };
    setConnErrors(errors);
    if (errors.host || errors.port) return;
    void run(
      { action: "adb_connect", host: connHost.trim(), port: Number(connPort.trim()) },
      "connect",
      setConnBusy,
      setConnOutcome,
    );
  };

  const titleId = `${uid}-title`;
  const pairOutId = `${uid}-pair-out`;
  const connOutId = `${uid}-conn-out`;
  const devOptId = `${uid}-devopt`;

  // Portaled to <body>: the sidebar's backdrop-filter would otherwise become
  // the containing block of the fixed scrim and pin the dialog inside it.
  return createPortal(
    // biome-ignore lint/a11y/noStaticElementInteractions: the scrim is a mouse-only "close" target; the keyboard path is Escape (handled on the dialog) and the Close button.
    <div
      className="fixed inset-0 z-50 flex items-center justify-center bg-bg/70 p-6"
      data-testid="add-device-scrim"
      onMouseDown={(e) => {
        if (e.target === e.currentTarget) onClose();
      }}
    >
      <div
        ref={dialogRef}
        role="dialog"
        aria-modal="true"
        aria-labelledby={titleId}
        className="glass box-border flex max-h-full w-[520px] max-w-full flex-col gap-4 overflow-auto px-[var(--pad-card-x)] py-[var(--pad-card-y)] text-fg"
        onKeyDown={onKeyDown}
      >
        <header className="flex items-center justify-between gap-3">
          <h2 id={titleId} className="m-0 text-title font-bold">
            Add a device
          </h2>
          <IconButton icon="close" label="Close" onClick={onClose} />
        </header>

        <form
          className="flex flex-col gap-2.5"
          aria-labelledby={`${uid}-pair-h`}
          aria-describedby={pairOutcome ? pairOutId : undefined}
          onSubmit={submitPair}
          noValidate
        >
          <h3 id={`${uid}-pair-h`} className="section-label m-0">
            Pair over Wi-Fi (Android 11+)
          </h3>
          <p className="m-0 text-body text-mut">
            On the device open Developer options → Wireless debugging → Pair device with pairing
            code, then copy the address and the code shown there.
          </p>
          <div className="flex gap-2.5">
            <Field
              id={`${uid}-pair-host`}
              label="Host"
              value={pairHost}
              onChange={setPairHost}
              placeholder="192.168.1.20"
              error={pairErrors.host}
              inputRef={firstFieldRef}
            />
            <Field
              id={`${uid}-pair-port`}
              label="Pairing port"
              value={pairPort}
              onChange={setPairPort}
              placeholder="37000"
              inputMode="numeric"
              maxLength={5}
              error={pairErrors.port}
            />
            <Field
              id={`${uid}-pair-code`}
              label="Pairing code"
              value={pairCode}
              onChange={setPairCode}
              placeholder="123456"
              inputMode="numeric"
              maxLength={6}
              error={pairErrors.code}
            />
          </div>
          <div className="flex items-center gap-3">
            <Button type="submit" variant="primary" disabled={pairBusy}>
              {pairBusy ? "Pairing…" : "Pair"}
            </Button>
            <OutcomeLine outcome={pairOutcome} id={pairOutId} />
          </div>
        </form>

        <form
          className="flex flex-col gap-2.5 border-edge border-t pt-4"
          aria-labelledby={`${uid}-conn-h`}
          aria-describedby={connOutcome ? connOutId : undefined}
          onSubmit={submitConnect}
          noValidate
        >
          <h3 id={`${uid}-conn-h`} className="section-label m-0">
            Connect
          </h3>
          <p className="m-0 text-body text-mut">
            After pairing, connect to the address under Wireless debugging (the port differs from
            the pairing port). Android 10 and older: run{" "}
            <span className="mono">adb tcpip 5555</span> over USB once, then connect to port 5555.
          </p>
          <div className="flex gap-2.5">
            <Field
              id={`${uid}-conn-host`}
              label="Host"
              value={connHost}
              onChange={setConnHost}
              placeholder="192.168.1.20"
              error={connErrors.host}
            />
            <Field
              id={`${uid}-conn-port`}
              label="Port"
              value={connPort}
              onChange={setConnPort}
              placeholder={String(DEFAULT_CONNECT_PORT)}
              inputMode="numeric"
              maxLength={5}
              error={connErrors.port}
            />
          </div>
          <div className="flex items-center gap-3">
            <Button type="submit" variant="primary" disabled={connBusy}>
              {connBusy ? "Connecting…" : "Connect"}
            </Button>
            <OutcomeLine outcome={connOutcome} id={connOutId} />
          </div>
        </form>

        <section
          aria-labelledby={`${uid}-usb-h`}
          className="flex flex-col gap-2 border-edge border-t pt-4"
        >
          <h3 id={`${uid}-usb-h`} className="section-label m-0">
            USB
          </h3>
          <p className="m-0 text-body text-mut">
            Plug the device in, enable USB debugging in Developer options, and accept the prompt on
            the device.
          </p>
          <button
            type="button"
            className="m-0 w-fit cursor-pointer border-0 bg-transparent p-0 text-body text-cyan underline underline-offset-2 focus-visible:outline-2 focus-visible:outline-cyan"
            aria-expanded={devOptionsOpen}
            aria-controls={devOptId}
            onClick={() => setDevOptionsOpen((v) => !v)}
          >
            Where is Developer options?
          </button>
          {devOptionsOpen ? (
            <p id={devOptId} className="m-0 text-body text-mut">
              Open Settings → About phone and tap Build number seven times; Developer options then
              appears under Settings → System. Turn on USB debugging there (and Wireless debugging
              for Wi-Fi).
            </p>
          ) : null}
        </section>

        <p className="m-0 border-edge border-t pt-4 text-body text-mut">
          iPhone and iPad connect over USB automatically (Phase 3).
        </p>
      </div>
    </div>,
    document.body,
  );
}

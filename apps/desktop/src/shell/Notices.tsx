import type { ErrorCode } from "@statasmon/protocol";
import { Button, IconButton, Pill, type Tone } from "@statasmon/ui";
import { useEffect, useState } from "react";
import { type AppleSupportStatus, commands } from "../bridge/commands";
import { type Notice, useAppStore } from "../store/app";

/** Notices that stay until the user dismisses them (they need an action outside the app). */
const PERSISTENT: ReadonlySet<ErrorCode> = new Set<ErrorCode>(["adb_not_found", "driver_missing"]);

/** Codes that mean "nothing is being read from that device right now". */
const SERIOUS: ReadonlySet<ErrorCode> = new Set<ErrorCode>([
  "adb_not_found",
  "device_lost",
  "tunnel_failed",
]);

/** Every other notice disappears on its own after this long. */
export const NOTICE_AUTO_DISMISS_MS = 12_000;

export function isPersistentNotice(code: ErrorCode): boolean {
  return PERSISTENT.has(code);
}

export function noticeTone(code: ErrorCode): Tone {
  return SERIOUS.has(code) ? "bad" : "warn";
}

/** `adb_not_found` → "adb not found". */
export function noticeCodeLabel(code: ErrorCode): string {
  return code.replace(/_/g, " ");
}

function errorText(e: unknown): string {
  if (e && typeof e === "object") {
    const o = e as { message?: unknown; hint?: unknown };
    const parts = [o.message, o.hint].filter((x): x is string => typeof x === "string");
    if (parts.length > 0) return parts.join(" ");
  }
  return typeof e === "string" ? e : "The install did not finish.";
}

type InstallState =
  | { kind: "idle" }
  | { kind: "running" }
  | { kind: "done" }
  | { kind: "failed"; text: string };

/**
 * "Install Apple device support" (ADR-022): shown on the driver-missing
 * notice when this computer can do it (Linux with a known package manager
 * and polkit). The command is shown before it runs; the system's own
 * dialog asks for the password, the app never sees it.
 */
function AppleSupportInstall({ onInstalled }: { onInstalled: () => void }) {
  const [status, setStatus] = useState<AppleSupportStatus | null>(null);
  const [state, setState] = useState<InstallState>({ kind: "idle" });

  useEffect(() => {
    let alive = true;
    commands
      .appleSupportStatus()
      .then((s) => {
        if (alive) setStatus(s);
      })
      .catch(() => undefined);
    return () => {
      alive = false;
    };
  }, []);

  if (!status?.installable || !status.command) return null;

  const install = async () => {
    setState({ kind: "running" });
    try {
      await commands.installAppleSupport();
      setState({ kind: "done" });
      onInstalled();
    } catch (e) {
      setState({ kind: "failed", text: errorText(e) });
    }
  };

  return (
    <span className="notice-install" data-testid="apple-support-install">
      <span className="text-mut">
        StatasMon can install it for you. Your system will ask for an administrator password;
        StatasMon never sees it. This runs:
      </span>
      <code className="mono notice-install-cmd">{status.command}</code>
      <span className="flex items-center gap-2">
        <Button
          size="sm"
          variant="primary"
          disabled={state.kind === "running"}
          aria-busy={state.kind === "running"}
          onClick={() => void install()}
        >
          {state.kind === "running" ? "Installing…" : "Install Apple device support"}
        </Button>
        {state.kind === "done" ? (
          <span role="status" className="text-mint">
            Installed. Reconnect the device.
          </span>
        ) : null}
      </span>
      {state.kind === "failed" ? (
        <span role="alert" className="text-red">
          {state.text}
        </span>
      ) : null}
    </span>
  );
}

function NoticeItem({ notice }: { notice: Notice }) {
  const dismiss = useAppStore((s) => s.dismissNotice);
  const persistent = isPersistentNotice(notice.code);

  useEffect(() => {
    if (persistent) return;
    const t = setTimeout(() => dismiss(notice.id), NOTICE_AUTO_DISMISS_MS);
    return () => clearTimeout(t);
  }, [persistent, notice.id, dismiss]);

  return (
    <li
      role="status"
      data-notice-code={notice.code}
      className="glass flex items-start gap-2.5 px-3.5 py-3 text-body"
    >
      <Pill tone={noticeTone(notice.code)} size="sm" className="mt-0.5 shrink-0">
        {noticeCodeLabel(notice.code)}
      </Pill>
      <span className="flex min-w-0 flex-1 flex-col gap-1">
        <span className="text-fg">{notice.message}</span>
        {notice.hint ? <span className="text-mut">{notice.hint}</span> : null}
        {notice.code === "driver_missing" ? (
          <AppleSupportInstall onInstalled={() => setTimeout(() => dismiss(notice.id), 4000)} />
        ) : null}
      </span>
      <IconButton
        icon="close"
        label="Dismiss"
        size={14}
        className="-mt-1 -mr-1 shrink-0"
        onClick={() => dismiss(notice.id)}
      />
    </li>
  );
}

/**
 * Fixed stack at the top-right of the main column showing core `error`
 * messages (store `notices`). Warnings go away after 12 s; "adb not found"
 * and "driver missing" stay until dismissed because they need the user to
 * install something.
 */
export function Notices() {
  const notices = useAppStore((s) => s.notices);
  return (
    <ul
      aria-label="Notices"
      data-count={notices.length}
      className="fixed top-page-y right-page-x z-40 m-0 flex w-[380px] max-w-[calc(100vw-var(--sidebar-w)-2*var(--pad-page-x))] list-none flex-col gap-2 p-0"
    >
      {notices.map((n) => (
        <NoticeItem key={n.id} notice={n} />
      ))}
    </ul>
  );
}

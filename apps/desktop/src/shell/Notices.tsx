import type { ErrorCode } from "@statasmon/protocol";
import { IconButton, Pill, type Tone } from "@statasmon/ui";
import { useEffect } from "react";
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

import { IconButton } from "@statasmon/ui";
import { type KeyboardEvent as ReactKeyboardEvent, type ReactNode, useEffect, useRef } from "react";
import { createPortal } from "react-dom";

const FOCUSABLE =
  'a[href], button:not([disabled]), input:not([disabled]), select:not([disabled]), textarea:not([disabled]), [tabindex]:not([tabindex="-1"])';

export interface DialogFrameProps {
  titleId: string;
  title: string;
  onClose: () => void;
  /** Element focused on open (defaults to the first focusable control). */
  initialFocus?: { current: HTMLElement | null };
  width?: number;
  testId?: string;
  children: ReactNode;
}

/**
 * Glass modal portaled to `<body>` (a card's backdrop-filter would otherwise
 * pin the fixed scrim inside it): focus trapped, Escape closes, focus returns
 * to the opener. The same pattern as the Add-device dialog.
 */
export function DialogFrame({
  titleId,
  title,
  onClose,
  initialFocus,
  width = 560,
  testId,
  children,
}: DialogFrameProps) {
  const dialogRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    const opener = document.activeElement as HTMLElement | null;
    const target =
      initialFocus?.current ?? dialogRef.current?.querySelector<HTMLElement>(FOCUSABLE) ?? null;
    target?.focus();
    return () => {
      opener?.focus?.();
    };
  }, [initialFocus]);

  // Escape closes even when focus fell out of the dialog (a control that
  // disabled itself while busy, a click on the scrim).
  useEffect(() => {
    const onEscape = (e: KeyboardEvent) => {
      if (e.key !== "Escape" || e.defaultPrevented) return;
      if (dialogRef.current?.contains(e.target as Node)) return; // handled by onKeyDown
      e.preventDefault();
      onClose();
    };
    document.addEventListener("keydown", onEscape);
    return () => document.removeEventListener("keydown", onEscape);
  }, [onClose]);

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

  return createPortal(
    // biome-ignore lint/a11y/noStaticElementInteractions: the scrim is a mouse-only "close" target; the keyboard path is Escape (handled on the dialog) and the Close button.
    <div
      className="sm-scrim fixed inset-0 z-50 flex items-center justify-center bg-bg/70 p-6"
      data-testid={testId}
      onMouseDown={(e) => {
        if (e.target === e.currentTarget) onClose();
      }}
    >
      <div
        ref={dialogRef}
        role="dialog"
        aria-modal="true"
        aria-labelledby={titleId}
        className="glass box-border flex max-h-full max-w-full flex-col gap-4 overflow-auto px-[var(--pad-card-x)] py-[var(--pad-card-y)] text-fg"
        style={{ width }}
        onKeyDown={onKeyDown}
      >
        <header className="flex items-center justify-between gap-3">
          <h2 id={titleId} className="m-0 text-title font-bold">
            {title}
          </h2>
          <IconButton icon="close" label="Close" onClick={onClose} />
        </header>
        {children}
      </div>
    </div>,
    document.body,
  );
}

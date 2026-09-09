import { useSyncExternalStore } from "react";

/**
 * Tiny shared flag for the "Export arrives in Phase 5" note: the TopBar
 * button and the ⌘/Ctrl+E shortcut (handled in AppShell) both raise it, and
 * the TopBar shows it for `EXPORT_NOTE_MS`.
 */
export const EXPORT_NOTE_MS = 2000;

let visible = false;
let timer: ReturnType<typeof setTimeout> | undefined;
const listeners = new Set<() => void>();

function notify() {
  for (const l of listeners) l();
}

export function showExportNote(): void {
  visible = true;
  if (timer) clearTimeout(timer);
  timer = setTimeout(() => {
    visible = false;
    timer = undefined;
    notify();
  }, EXPORT_NOTE_MS);
  notify();
}

/** Test helper: hides the note immediately. */
export function resetExportNote(): void {
  visible = false;
  if (timer) clearTimeout(timer);
  timer = undefined;
  notify();
}

function subscribe(cb: () => void) {
  listeners.add(cb);
  return () => {
    listeners.delete(cb);
  };
}

export function useExportNote(): boolean {
  return useSyncExternalStore(subscribe, () => visible);
}

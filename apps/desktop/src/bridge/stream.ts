import type { Message } from "@statasmon/protocol";

/**
 * The one way the UI talks to the core. Inside Tauri this is a Channel plus
 * commands; in a plain browser (Playwright, `pnpm web:mock`) it replays a
 * fixture so the UI is testable without the Rust core.
 */
export interface DeviceStream {
  readonly kind: "tauri" | "replay";
  /** Starts delivery; returns an unsubscribe function. */
  subscribe(onMessage: (m: Message) => void): () => void;
  invoke<T>(command: string, args?: Record<string, unknown>): Promise<T>;
}

export function isTauri(): boolean {
  return typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;
}

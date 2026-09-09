import type { LogFilter } from "@statasmon/protocol";
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { commands } from "../../bridge/commands";
import {
  EMPTY_STORED,
  type Narrowing,
  nextStoredPage,
  STORED_PAGE,
  type StoredPages,
  storedFilter,
  storedFilterKey,
} from "./logsModel";

function describe(e: unknown): string {
  if (e && typeof e === "object" && typeof (e as { message?: unknown }).message === "string") {
    return (e as { message: string }).message;
  }
  return typeof e === "string" ? e : "The store could not be queried";
}

export interface StoredPaging {
  state: StoredPages;
  /** The query the pages come from (undefined while paging is disabled). */
  filter: LogFilter | undefined;
  /** Loads the page just before what is loaded (the newest matches first). */
  loadOlder(): void;
  /** True once the first page arrived and nothing older remains. */
  exhausted: boolean;
}

/**
 * "Stored session" paging for the Logs screen (docs/PLAN.md 6.4): pages
 * `commands.queryLogs` backwards from the end of the store's matches so each
 * click prepends the 500 lines just before the ones already shown. The query
 * is the UI filter plus the narrowing, bounded to lines older than the live
 * ring's oldest line; that bound is frozen while pages are loaded so the ring
 * dropping lines does not restart the paging. The first click probes the
 * total with a one-line query, then fetches the last page. A different query
 * restarts from nothing.
 */
export function useStoredPages(
  base: LogFilter | undefined,
  narrowing: Narrowing,
  oldestLiveMs: number | undefined,
  enabled: boolean,
): StoredPaging {
  const [state, setState] = useState<StoredPages>(EMPTY_STORED);
  const frozen = useRef(oldestLiveMs);
  if (state.entries.length === 0) frozen.current = oldestLiveMs;
  const bound = frozen.current;
  const filter = useMemo(
    () => (base && enabled ? storedFilter(base, narrowing, bound) : undefined),
    [base, narrowing, bound, enabled],
  );
  const key = filter ? storedFilterKey(filter) : undefined;
  const filterRef = useRef(filter);
  filterRef.current = filter;
  const keyRef = useRef(key);
  keyRef.current = key;
  const stateRef = useRef(state);
  stateRef.current = state;

  // A new query forgets the pages of the previous one.
  // biome-ignore lint/correctness/useExhaustiveDependencies: `key` is the content key for `filter`
  useEffect(() => {
    setState(EMPTY_STORED);
  }, [key]);

  const loadOlder = useCallback(() => {
    const f = filterRef.current;
    const k = keyRef.current;
    const s = stateRef.current;
    if (!f || k === undefined || s.loading) return;
    const page = nextStoredPage(s);
    if (!page) return;
    const next = { ...s, loading: true, error: undefined };
    stateRef.current = next;
    setState(next);
    void (async () => {
      try {
        let total = s.total;
        let want = page;
        if (total === undefined) {
          const probe = await commands.queryLogs(f, 0, 1);
          total = probe?.total ?? 0;
          const offset = Math.max(0, total - STORED_PAGE);
          want = { offset, limit: total - offset };
        }
        const result =
          want.limit > 0
            ? await commands.queryLogs(f, want.offset, want.limit)
            : { total, first_seq: 0, entries: [] };
        if (keyRef.current !== k) return;
        const entries = result?.entries ?? [];
        setState((cur) => ({
          entries: entries.concat(cur.entries),
          offset: want.offset,
          total: result?.total ?? total,
          loading: false,
        }));
      } catch (e) {
        if (keyRef.current !== k) return;
        setState((cur) => ({ ...cur, loading: false, error: describe(e) }));
      }
    })();
  }, []);

  const exhausted = state.total !== undefined && nextStoredPage(state) === undefined;
  return { state, filter, loadOlder, exhausted };
}

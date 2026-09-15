import { cx } from "@statasmon/ui";
import { type ReactNode, useLayoutEffect, useRef } from "react";

/** Row unit of the underlying grid (px); heights round up to it. */
const UNIT = 2;

export interface MasonryProps {
  children: ReactNode;
  /** Narrowest column before dropping a column. */
  minColumnWidth?: number;
  maxColumns?: number;
  /** Gap between cards, both axes (px). */
  gap?: number;
  className?: string;
}

/**
 * Packs its children into columns: the first child stays top-left, the
 * others go tallest first into the shortest column, so short and tall cards
 * never leave a hole beside each other and the columns end level. Implemented on a CSS grid with 2 px rows: each
 * child gets an explicit column and row span from its measured height, and
 * a ResizeObserver re-packs when content or width changes.
 */
export function Masonry({
  children,
  minColumnWidth = 400,
  maxColumns = 3,
  gap = 14,
  className,
}: MasonryProps) {
  const ref = useRef<HTMLDivElement>(null);

  useLayoutEffect(() => {
    const el = ref.current;
    if (!el) return undefined;
    const pack = () => layout(el, minColumnWidth, maxColumns, gap);
    pack();
    if (typeof ResizeObserver === "undefined") return undefined;
    const ro = new ResizeObserver(pack);
    ro.observe(el);
    for (const child of el.children) ro.observe(child);
    const mo = new MutationObserver(() => {
      for (const child of el.children) ro.observe(child);
      pack();
    });
    mo.observe(el, { childList: true });
    return () => {
      ro.disconnect();
      mo.disconnect();
    };
  }, [minColumnWidth, maxColumns, gap]);

  return (
    <div
      ref={ref}
      className={cx("sm-masonry", className)}
      style={{ columnGap: gap, gridAutoRows: UNIT }}
      data-testid="masonry"
    >
      {children}
    </div>
  );
}

/** Columns that fit, then shortest-column placement in document order. */
export function layout(el: HTMLElement, minColumnWidth: number, maxColumns: number, gap: number) {
  const width = el.clientWidth || minColumnWidth;
  const columns = Math.max(
    1,
    Math.min(maxColumns, Math.floor((width + gap) / (minColumnWidth + gap))),
  );
  el.style.gridTemplateColumns = `repeat(${columns}, minmax(0, 1fr))`;
  el.dataset.columns = String(columns);
  const gapRows = Math.ceil(gap / UNIT);
  const bottoms = new Array<number>(columns).fill(0); // in rows
  const children = [...(el.children as HTMLCollectionOf<HTMLElement>)];
  // The first card keeps the top-left; the rest go tallest first into the
  // shortest column (longest-processing-time scheduling), which balances
  // the columns far better than document order when one card is huge.
  const [first, ...rest] = children;
  const ordered = first ? [first, ...rest.sort((a, b) => b.offsetHeight - a.offsetHeight)] : [];
  for (const child of ordered) {
    const span = Math.max(1, Math.ceil(child.offsetHeight / UNIT));
    let col = 0;
    for (let i = 1; i < columns; i += 1) if (bottoms[i] < bottoms[col]) col = i;
    const start = bottoms[col] + (bottoms[col] === 0 ? 0 : gapRows);
    child.style.gridColumn = String(col + 1);
    child.style.gridRow = `${start + 1} / span ${span}`;
    bottoms[col] = start + span;
  }
}

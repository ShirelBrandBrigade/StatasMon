import type { CoreKind } from "@statasmon/protocol";
import type { CSSProperties, HTMLAttributes } from "react";
import { clamp, cx, seriesColor } from "./tokens";

export interface CoreBar {
  index: number;
  kind: CoreKind;
  /** 0–100; `null` when the platform does not report per-core usage. */
  usagePct: number | null;
  online: boolean;
}

export interface CoreBarsProps extends HTMLAttributes<HTMLDivElement> {
  cores: CoreBar[];
}

const PREFIX: Record<CoreKind, string> = { performance: "P", efficiency: "E", unknown: "C" };

/** Label such as "P0", "E2" or "C5"; numbering restarts per kind. */
export function coreLabel(core: CoreBar, cores: CoreBar[]): string {
  const ordinal = cores.filter((c) => c.kind === core.kind && c.index < core.index).length;
  return `${PREFIX[core.kind]}${ordinal}`;
}

/** One 44px bar per core: performance in cyan, efficiency in cyan-2, labelled P0… / E0…. */
export function CoreBars({ cores, className, ...rest }: CoreBarsProps) {
  const sorted = [...cores].sort((a, b) => a.index - b.index);
  return (
    <div
      className={cx("sm-cores", className)}
      style={{ "--cores": sorted.length } as CSSProperties}
      {...rest}
    >
      <div className="sm-cores-bars">
        {sorted.map((c) => {
          const label = coreLabel(c, sorted);
          const pct = c.usagePct === null ? 0 : clamp(c.usagePct, 0, 100);
          const text = c.usagePct === null ? "unavailable" : `${Math.round(pct)}%`;
          return (
            <i
              key={c.index}
              className={cx(!c.online && "is-offline")}
              role="img"
              aria-label={`${label} ${c.online ? text : "offline"}`}
              title={`${label} · ${c.online ? text : "offline"}`}
            >
              <b
                style={{
                  height: `${pct}%`,
                  background: seriesColor(c.kind === "efficiency" ? "cyan-2" : "cyan"),
                }}
              />
            </i>
          );
        })}
      </div>
      <div className="sm-cores-labels mono" aria-hidden>
        {sorted.map((c) => (
          <span key={c.index}>{coreLabel(c, sorted)}</span>
        ))}
      </div>
    </div>
  );
}

import {
  AppTile,
  Button,
  Card,
  CardTitle,
  CoreBars,
  Dash,
  ICON_NAMES,
  Icon,
  IconButton,
  KeyValue,
  Legend,
  LevelPill,
  MetricFigure,
  Pill,
  Ring,
  Segment,
  Sparkline,
  StackBar,
  StatusDot,
} from "@statasmon/ui";
import { type ReactNode, useState } from "react";

/** Section names, exported so the test can assert every primitive is on the page. */
export const GALLERY_SECTIONS = [
  "Card",
  "Ring",
  "StackBar + Legend",
  "CoreBars",
  "KeyValue",
  "Pill + LevelPill",
  "Sparkline",
  "Button + IconButton",
  "Segment",
  "AppTile + StatusDot",
  "Icon",
  "MetricFigure",
  "Dash",
] as const;

type SectionName = (typeof GALLERY_SECTIONS)[number];

function Section({ name, children }: { name: SectionName; children: ReactNode }) {
  return (
    <section
      aria-labelledby={`gallery-${name}`}
      style={{ display: "flex", flexDirection: "column", gap: "var(--gap-stack)" }}
    >
      <h2 id={`gallery-${name}`} className="section-label" style={{ margin: 0 }}>
        {name}
      </h2>
      {children}
    </section>
  );
}

const ROW = {
  display: "flex",
  gap: "var(--gap-card)",
  flexWrap: "wrap",
  alignItems: "center",
} as const;
const RING_ROW = { display: "flex", alignItems: "center", gap: 14 } as const;

const CORES = [
  { index: 0, kind: "performance", usagePct: 62, online: true },
  { index: 1, kind: "performance", usagePct: 58, online: true },
  { index: 2, kind: "efficiency", usagePct: 31, online: true },
  { index: 3, kind: "efficiency", usagePct: 27, online: true },
  { index: 4, kind: "efficiency", usagePct: 24, online: true },
  { index: 5, kind: "efficiency", usagePct: 19, online: true },
] as const;

const RANGES = [
  { value: "live", label: "Live" },
  { value: "5m", label: "5 min" },
  { value: "15m", label: "15 min" },
  { value: "1h", label: "1 h" },
  { value: "session", label: "Session" },
] as const;

const DOWN = [15, 28, 22, 45, 38, 60, 54, 70, 48, 64, 76, 56, 62, 50, 58];
const THERMAL = [6, 8, 10, 16, 20, 24, 30, 34, 36, 38, 38];

/** Every primitive with representative props, on the app mesh, for visual review against AD-1. */
export function Gallery() {
  const [range, setRange] = useState<(typeof RANGES)[number]["value"]>("5m");
  return (
    <main
      className="app-mesh"
      style={{
        minHeight: "100vh",
        padding: "var(--pad-page-y) var(--pad-page-x)",
        display: "flex",
        flexDirection: "column",
        gap: 26,
      }}
    >
      <header>
        <h1
          style={{
            margin: 0,
            fontSize: "var(--fs-headline)",
            fontWeight: 800,
            letterSpacing: "-0.02em",
          }}
        >
          UI gallery
        </h1>
        <p className="mt-1 mb-0 text-mut text-support">
          Every primitive from @statasmon/ui with representative props, on the app mesh.
        </p>
        <p className="mt-2 mb-0 inline-block rounded-ctl border border-edge bg-card px-3 py-1 font-mono text-label tabular-nums text-dim">
          Tailwind theme check: bg-card · border-edge · rounded-ctl · font-mono · text-label
        </p>
      </header>

      <Section name="Card">
        <div
          style={{
            display: "grid",
            gridTemplateColumns: "repeat(3, minmax(0, 1fr))",
            gap: "var(--gap-card)",
          }}
        >
          <Card title="CPU" subtitle="A18 Pro · 2P + 4E">
            <p style={{ margin: 0, fontSize: "var(--fs-body)", color: "var(--mut)" }}>
              Card with title and subtitle.
            </p>
          </Card>
          <Card title="Thermal" subtitle={<Pill tone="warn">Fair · 11 min</Pill>}>
            <p style={{ margin: 0, fontSize: "var(--fs-body)", color: "var(--mut)" }}>
              Subtitle slot holding a Pill.
            </p>
          </Card>
          <Card
            title="Log"
            footer={
              <>
                <span>2 148 entries · following</span>
                <span>·</span>
                <span>11 errors · 1 fault</span>
              </>
            }
          >
            <p style={{ margin: 0, fontSize: "var(--fs-body)", color: "var(--mut)" }}>
              Card with a footer.
            </p>
          </Card>
        </div>
        <Card>
          <CardTitle title="Standalone CardTitle" subtitle="right-aligned subtitle" />
        </Card>
      </Section>

      <Section name="Ring">
        <Card>
          <div style={ROW}>
            <Ring value={38} name="CPU" series="cyan" />
            <Ring value={68} name="Memory" series="vio" />
            <Ring value={77} name="Storage" series="mint" />
            <Ring value={87} name="Battery" series="amber" />
            <Ring value={null} name="Unavailable" />
          </div>
        </Card>
      </Section>

      <Section name="StackBar + Legend">
        <Card title="Memory" subtitle="8 GB · pressure normal">
          <StackBar
            name="Memory by kind"
            total={8}
            segments={[
              { label: "App", value: 3.1, series: "cyan" },
              { label: "Wired", value: 1.4, series: "vio" },
              { label: "Compressed", value: 0.9, series: "amber" },
            ]}
          />
          <Legend
            items={[
              { label: "App", value: "3.1 GB", series: "cyan" },
              { label: "Wired", value: "1.4 GB", series: "vio" },
              { label: "Compr.", value: "0.9 GB", series: "amber" },
            ]}
          />
        </Card>
      </Section>

      <Section name="CoreBars">
        <Card title="CPU" subtitle="A18 Pro · 2P + 4E">
          <div style={RING_ROW}>
            <Ring value={38} name="CPU" />
            <CoreBars cores={[...CORES]} />
          </div>
        </Card>
      </Section>

      <Section name="KeyValue">
        <Card title="Battery" subtitle="charging">
          <KeyValue
            rows={[
              { label: "Voltage", value: "4.21 V" },
              { label: "Current", value: "+1 120 mA" },
              { label: "Health", value: "96% · 214 cyc", tone: "ok" },
              { label: "Full in", value: "38 min" },
              { label: "Temperature", value: <Dash reason="Not reported over this transport" /> },
            ]}
          />
        </Card>
      </Section>

      <Section name="Pill + LevelPill">
        <Card>
          <div style={ROW}>
            <Pill tone="ok">Nominal</Pill>
            <Pill tone="warn">Fair · 11 min</Pill>
            <Pill tone="bad">Critical</Pill>
            <Pill tone="info">USB-C</Pill>
            <Pill tone="note">Notice</Pill>
            <Pill tone="neutral">Idle</Pill>
          </div>
          <div style={ROW}>
            <LevelPill level="debug" />
            <LevelPill level="info" />
            <LevelPill level="notice" />
            <LevelPill level="warning" />
            <LevelPill level="error" />
            <LevelPill level="fault" />
          </div>
        </Card>
      </Section>

      <Section name="Sparkline">
        <Card title="Network" subtitle="Wi-Fi Studio-5G · −58 dBm · 18 ms">
          <Sparkline
            values={DOWN}
            width={500}
            height={90}
            series="cyan"
            fill
            endpoint
            name="Download"
          />
          <Sparkline
            values={THERMAL}
            width={300}
            height={50}
            series="amber"
            name="SoC temperature"
          />
        </Card>
      </Section>

      <Section name="Button + IconButton">
        <Card>
          <div style={ROW}>
            <Button variant="primary" icon="download">
              Export
            </Button>
            <Button variant="ghost">Pause</Button>
            <Button variant="ghost" size="sm" icon="mark">
              Mark
            </Button>
            <Button variant="primary" size="sm" disabled>
              Disabled
            </Button>
            <IconButton icon="close" label="Close" />
            <IconButton icon="search" label="Search" />
            <IconButton icon="plus" label="Add a device" />
          </div>
        </Card>
      </Section>

      <Section name="Segment">
        <Card>
          <div style={ROW}>
            <Segment name="Range" options={RANGES} value={range} onChange={setRange} />
            <span className="mono" style={{ fontSize: "var(--fs-body)", color: "var(--mut)" }}>
              selected: {range}
            </span>
          </div>
        </Card>
      </Section>

      <Section name="AppTile + StatusDot">
        <Card>
          <div style={ROW}>
            <AppTile me label="FieldKit (debug)" />
            <AppTile label="Safari" />
            <AppTile label="kernel_task" />
            <span style={{ width: 14 }} />
            <StatusDot tone="ok" label="Connected" />{" "}
            <span style={{ fontSize: "var(--fs-body)" }}>Connected</span>
            <StatusDot tone="warn" label="Warm" />{" "}
            <span style={{ fontSize: "var(--fs-body)" }}>Warm</span>
            <StatusDot tone="bad" label="Recording" glow />{" "}
            <span style={{ fontSize: "var(--fs-body)" }}>Recording</span>
            <StatusDot tone="neutral" label="Idle" />{" "}
            <span style={{ fontSize: "var(--fs-body)" }}>Idle</span>
          </div>
        </Card>
      </Section>

      <Section name="Icon">
        <Card>
          <div style={{ ...ROW, gap: 18 }}>
            {ICON_NAMES.map((name) => (
              <span
                key={name}
                style={{
                  display: "inline-flex",
                  flexDirection: "column",
                  alignItems: "center",
                  gap: 6,
                  width: 72,
                }}
              >
                <Icon name={name} size={20} label={name} />
                <span className="mono" style={{ fontSize: "var(--fs-axis)", color: "var(--dim)" }}>
                  {name}
                </span>
              </span>
            ))}
          </div>
        </Card>
      </Section>

      <Section name="MetricFigure">
        <Card>
          <div style={{ ...ROW, gap: 22, alignItems: "flex-start" }}>
            <MetricFigure
              meta={{ unit: "mbps" }}
              value={24.6}
              suffix="down"
              caption="1 842 pkt/s · 1.8 GB"
              series="cyan"
              size="lg"
            />
            <MetricFigure
              meta={{ unit: "mbps" }}
              value={3.2}
              suffix="up"
              caption="611 pkt/s · 214 MB"
              series="vio"
              size="lg"
            />
            <MetricFigure meta={{ unit: "bytes" }} value={5.4 * 1024 ** 3} suffix="used" />
            <MetricFigure meta={{ unit: "percent" }} value={87} caption="charging" />
            <MetricFigure meta={{ unit: "celsius" }} value={41.2} caption="SoC" />
            <MetricFigure meta={{ unit: "count" }} value={2148} caption="entries" />
            <MetricFigure
              meta={{ unit: "watts" }}
              value={null}
              unavailableReason="Power draw is not exposed by this device"
            />
          </div>
        </Card>
      </Section>

      <Section name="Dash">
        <Card>
          <p style={{ margin: 0, fontSize: "var(--fs-body)" }}>
            Per-process network: <Dash reason="iOS does not report per-process network counters" />{" "}
            · hover for the reason.
          </p>
        </Card>
      </Section>
    </main>
  );
}

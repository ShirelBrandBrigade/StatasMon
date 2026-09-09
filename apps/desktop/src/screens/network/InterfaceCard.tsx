import { formatValue, type Unit } from "@statasmon/protocol";
import { Card, Dash, KeyValue, type KeyValueRow } from "@statasmon/ui";
import type { ReactNode } from "react";
import { useCapability, useDevice, useLatest } from "../../hooks/useDevice";
import { Subtitle } from "../../overview/cards/shared";
import { transportLabel } from "../../shell/format";
import { NOT_IN_CONTRACT, why } from "./why";

const DBM = { unit: "dbm" } as const satisfies { unit: Unit };
const MBPS = { unit: "mbps" } as const satisfies { unit: Unit };
const MS = { unit: "milliseconds" } as const satisfies { unit: Unit };
const COUNT = { unit: "count" } as const satisfies { unit: Unit };

/** Typographic minus for negative readings (dBm). */
export const minus = (text: string) => text.replace(/^-/, "−");

/** Interface / Wi-Fi: signal, link speed, connections, retransmits, dropped, latency. */
export function InterfaceCard({ deviceId }: { deviceId: string }) {
  const summary = useDevice(deviceId)?.summary;
  const rssi = useLatest(deviceId, "net.rssi");
  const link = useLatest(deviceId, "net.link_mbps");
  const connections = useLatest(deviceId, "net.connections");
  const retransmits = useLatest(deviceId, "net.retransmits");
  const dropped = useLatest(deviceId, "net.dropped");
  const latency = useLatest(deviceId, "net.latency_ms");
  const wifi = useCapability(deviceId, "wifi_details");
  const throughput = useCapability(deviceId, "network_throughput");
  const perProcess = useCapability(deviceId, "network_per_process");

  const val = (v: number | undefined, text: string, reason: string): ReactNode =>
    v !== undefined ? text : <Dash reason={reason} />;
  const wifiReason = why(wifi);
  const rows: KeyValueRow[] = [
    { label: "Signal (RSSI)", value: val(rssi, minus(formatValue(DBM, rssi)), wifiReason) },
    { label: "Link speed", value: val(link, formatValue(MBPS, link), wifiReason) },
    {
      label: "SSID",
      value: <Dash reason={wifi.available === false ? wifiReason : NOT_IN_CONTRACT} />,
    },
    {
      label: "IP address",
      value: <Dash reason={wifi.available === false ? wifiReason : NOT_IN_CONTRACT} />,
    },
    {
      label: "Connections",
      value: val(connections, formatValue(COUNT, connections), why(perProcess)),
    },
    {
      label: "Retransmits",
      value: val(retransmits, formatValue(COUNT, retransmits), why(perProcess)),
    },
    { label: "Dropped", value: val(dropped, formatValue(COUNT, dropped), why(perProcess)) },
    { label: "Latency", value: val(latency, formatValue(MS, latency), why(throughput)) },
  ];

  return (
    <Card
      title="Interface"
      className="net-card net-interface"
      data-testid="net-interface"
      subtitle={
        <Subtitle
          deviceId={deviceId}
          parts={[summary ? `host link ${transportLabel(summary.transport)}` : undefined]}
        />
      }
    >
      <KeyValue rows={rows} data-testid="net-interface-kv" />
    </Card>
  );
}

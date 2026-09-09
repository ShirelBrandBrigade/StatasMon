import { Card } from "@statasmon/ui";
import type { ComponentType } from "react";
import { TopBar } from "../shell/TopBar";
import { useAppStore } from "../store/app";

/** The eight card slots of the Overview, in mockup order. */
export const SLOT_NAMES = [
  "cpu",
  "memory",
  "storage",
  "battery",
  "network",
  "thermal",
  "apps",
  "log",
] as const;

export type SlotName = (typeof SLOT_NAMES)[number];

/** Every slot component receives the active device id and renders its own card. */
export type SlotComponent = ComponentType<{ deviceId: string }>;

export type OverviewSlots = Partial<Record<SlotName, SlotComponent>>;

const SLOT_TITLE: Record<SlotName, string> = {
  cpu: "CPU",
  memory: "Memory",
  storage: "Storage",
  battery: "Battery",
  network: "Network",
  thermal: "Thermal",
  apps: "Top apps",
  log: "Log",
};

function Slot({
  name,
  deviceId,
  component: Component,
}: {
  name: SlotName;
  deviceId: string;
  component?: SlotComponent;
}) {
  if (Component) {
    return (
      <div className="ov-slot" data-slot={name}>
        <Component deviceId={deviceId} />
      </div>
    );
  }
  return (
    <Card
      className="ov-slot ov-placeholder"
      data-slot={name}
      title={<span className="ov-placeholder-title">{SLOT_TITLE[name]}</span>}
      subtitle="Card in progress"
    />
  );
}

/**
 * Overview page: headline row, four metric cards, network · thermal · apps,
 * then the log card filling the rest. Pass `slots` to replace placeholders
 * with the real cards; each receives `{ deviceId }` of the active device.
 */
export function Overview({ slots = {} }: { slots?: OverviewSlots }) {
  const deviceId = useAppStore((s) => s.activeDeviceId);
  if (!deviceId) return null;
  const slot = (name: SlotName) => (
    <Slot key={name} name={name} deviceId={deviceId} component={slots[name]} />
  );
  return (
    <>
      <TopBar deviceId={deviceId} />
      <div className="ov-cards">{(["cpu", "memory", "storage", "battery"] as const).map(slot)}</div>
      <div className="ov-row2">{(["network", "thermal", "apps"] as const).map(slot)}</div>
      {slot("log")}
    </>
  );
}

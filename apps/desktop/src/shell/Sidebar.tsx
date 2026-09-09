import { Button, cx, Icon } from "@statasmon/ui";
import { useShallow } from "zustand/react/shallow";
import { commands } from "../bridge/commands";
import { useAppStore } from "../store/app";
import { DeviceCard } from "./DeviceCard";
import { forgetDetached, useDetached } from "./detached";
import { deviceIcon } from "./format";
import { RecordingCard } from "./RecordingCard";
import { hashForView, VIEWS, type ViewId } from "./routes";

export function Sidebar({ view }: { view: ViewId }) {
  const { order, devices, activeDeviceId, setActive } = useAppStore(
    useShallow((s) => ({
      order: s.order,
      devices: s.devices,
      activeDeviceId: s.activeDeviceId,
      setActive: s.setActive,
    })),
  );
  const detached = useDetached().filter((d) => !(d.id in devices));

  return (
    <aside className="side">
      <div className="brand">
        <i aria-hidden />
        StatasMon
      </div>

      <div className="side-group">
        <h2 className="section-label side-h">Devices</h2>
        <ul className="dev-list" aria-label="Devices">
          {order.map((id) => {
            const entry = devices[id];
            return entry ? (
              <li key={id}>
                <DeviceCard entry={entry} active={id === activeDeviceId} onSelect={setActive} />
              </li>
            ) : null;
          })}
          {detached.map((d) => (
            <li key={d.id} className="dev is-detached">
              <span className="dev-ic">
                <Icon name={deviceIcon(d)} size={16} />
              </span>
              <span className="dev-txt">
                <b>{d.name}</b>
                <span>unplugged</span>
              </span>
              <Button
                size="sm"
                onClick={() => {
                  forgetDetached(d.id);
                  void commands.fakeReattach(d.id);
                }}
              >
                Reattach
              </Button>
            </li>
          ))}
        </ul>
        <button
          type="button"
          className="add-device"
          title="Real devices arrive in Phase 2"
          aria-describedby="add-device-hint"
        >
          <Icon name="plus" size={12} strokeWidth={3} />
          Add a device
        </button>
        <span id="add-device-hint" hidden>
          Real devices arrive in Phase 2
        </span>
      </div>

      <nav className="side-group" aria-label="Views">
        <h2 className="section-label side-h">Views</h2>
        <div className="nav-list">
          {VIEWS.map((v) => (
            <a
              key={v.id}
              href={hashForView(v.id)}
              className={cx("nav-item", v.id === view && "is-on")}
              aria-current={v.id === view ? "page" : undefined}
            >
              <Icon name={v.icon} size={15} />
              {v.label}
            </a>
          ))}
        </div>
      </nav>

      {activeDeviceId ? <RecordingCard deviceId={activeDeviceId} /> : null}
    </aside>
  );
}

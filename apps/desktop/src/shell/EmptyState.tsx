import { Card, Pill } from "@statasmon/ui";
import { useAppStore } from "../store/app";
import { isPersistentNotice, noticeCodeLabel, noticeTone } from "./Notices";

/**
 * Shown in the main column when no device is attached. Persistent notices
 * ("adb not found", "driver missing") are repeated here with their hint so
 * the reason nothing appears is readable without a device.
 */
export function EmptyState() {
  const persistent = useAppStore((s) => s.notices).filter((n) => isPersistentNotice(n.code));
  return (
    <div className="empty">
      <Card className="empty-card" aria-labelledby="empty-title">
        <h1 id="empty-title">No device connected</h1>
        <p>
          Plug in an iPhone, iPad or Android device and it will appear in the list on the left.
          Nothing leaves this computer: everything recorded stays in your app-data folder.
        </p>
        {persistent.length > 0 ? (
          <ul aria-label="Blocking problems" className="m-0 mt-4 flex list-none flex-col gap-2 p-0">
            {persistent.map((n) => (
              <li
                key={n.id}
                data-notice-code={n.code}
                className="flex flex-col items-center gap-1 rounded-inner border border-edge bg-card px-3.5 py-3 text-left text-body"
              >
                <span className="flex items-center gap-2">
                  <Pill tone={noticeTone(n.code)} size="sm">
                    {noticeCodeLabel(n.code)}
                  </Pill>
                  <span className="text-fg">{n.message}</span>
                </span>
                {n.hint ? <span className="text-mut">{n.hint}</span> : null}
              </li>
            ))}
          </ul>
        ) : null}
      </Card>
    </div>
  );
}

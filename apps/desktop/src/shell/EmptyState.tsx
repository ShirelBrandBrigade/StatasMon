import { Card } from "@statasmon/ui";

/** Shown in the main column when no device is attached. */
export function EmptyState() {
  return (
    <div className="empty">
      <Card className="empty-card" aria-labelledby="empty-title">
        <h1 id="empty-title">No device connected</h1>
        <p>
          Plug in an iPhone, iPad or Android device and it will appear in the list on the left.
          Nothing leaves this computer: everything recorded stays in your app-data folder.
        </p>
      </Card>
    </div>
  );
}

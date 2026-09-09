import { Card } from "@statasmon/ui";
import { VIEWS, type ViewId } from "./routes";

/** Placeholder page for views that arrive in Phase 6. */
export function ComingSoon({ view }: { view: ViewId }) {
  const label = VIEWS.find((v) => v.id === view)?.label ?? view;
  return (
    <div className="empty">
      <Card className="empty-card" aria-labelledby="coming-title">
        <h1 id="coming-title">{label}</h1>
        <p>Coming in Phase 6</p>
      </Card>
    </div>
  );
}

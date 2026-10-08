import type { ActionError } from "../hooks/useCoreSnapshot";
import { Button } from "../ui/Button";

export function ErrorBanner({
  error,
  onDismiss,
}: {
  error: ActionError | null;
  onDismiss(): void;
}) {
  if (!error) return null;
  return (
    <div
      role="alert"
      className="mx-3 mt-2 flex items-center justify-between gap-3 rounded bg-danger-bg px-2.5 py-2 text-sm text-danger-fg"
    >
      <span>{error.message}</span>
      <Button aria-label="Dismiss error" onClick={onDismiss}>
        ×
      </Button>
    </div>
  );
}

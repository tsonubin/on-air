import { API_BASE, type FirewallWarning as Warning } from "@on-air/api-types";
import { repairFirewall } from "@on-air/control-client";
import { useState } from "react";
import { Button } from "../ui/Button";

export function FirewallWarning({
  warning,
  onRepaired,
}: {
  warning?: Warning;
  onRepaired(): void;
}) {
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  if (!warning) return null;
  const repair = async () => {
    if (busy) return;
    setBusy(true);
    setError(null);
    try {
      await repairFirewall(API_BASE);
      onRepaired();
    } catch {
      setError("The rule was not applied. Authorize on this computer or run the commands below.");
    } finally {
      setBusy(false);
    }
  };
  return (
    <aside
      role="alert"
      className="mx-3 mt-2 rounded border border-line bg-face-2 p-3 text-sm text-amber"
    >
      <strong>{warning.message}</strong>
      <details className="mt-2">
        <summary className="cursor-pointer">Firewall help</summary>
        <p>
          Allow devices on {warning.subnet} to reach on-air on TCP port {warning.port}. This
          requires administrator authentication on this computer.
        </p>
        <Button disabled={busy} aria-busy={busy} onClick={() => void repair()}>
          {busy ? "Awaiting administrator…" : "Allow LAN audio"}
        </Button>
        <p>Or run these commands on the computer running on-air:</p>
        <pre className="overflow-x-auto whitespace-pre-wrap break-all select-text">
          {warning.repair_command}
        </pre>
        {error && <p role="alert">{error}</p>}
      </details>
    </aside>
  );
}

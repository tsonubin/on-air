import type { AirPlayMode, StatusResponse } from "@on-air/api-types";
import { DEFAULT_PORT } from "@on-air/api-types";
import { Button } from "../ui/Button";

export interface StatusPopoverProps {
  status: StatusResponse | null;
  paused: boolean;
  airplayMode: AirPlayMode | null;
  autostart: boolean | null;
  onToggleAutostart(): void;
}

export function StatusPopover({
  status,
  paused,
  airplayMode,
  autostart,
  onToggleAutostart,
}: StatusPopoverProps) {
  return (
    <details className="relative">
      <summary
        aria-label="more status"
        className="hatch-knob size-[18px] cursor-pointer rounded-full border border-face hover:brightness-110"
      />
      <div className="panel-raised absolute top-[calc(100%+8px)] right-0 z-30 flex min-w-[7.5rem] flex-col gap-1.5 rounded-b border border-line px-2.5 py-2 font-mono text-sm text-ink">
        <span>v{status?.version ?? "—"}</span>
        <span>{paused ? "service paused" : "service on"}</span>
        <span>:{DEFAULT_PORT}</span>
        <span data-testid="airplay-mode">{airplayMode ?? "—"}</span>
        {autostart !== null && (
          <Button
            variant="ghost"
            data-testid="autostart-hint"
            aria-pressed={autostart}
            onClick={onToggleAutostart}
          >
            {autostart ? "open at login" : "manual start"}
          </Button>
        )}
      </div>
    </details>
  );
}

import type { ActiveOutput, StatusResponse } from "@on-air/api-types";
import { DEFAULT_PORT } from "@on-air/api-types";
import type { ConnectionState } from "@on-air/control-client";
import type { Connection } from "../hooks/useCoreSnapshot";
import { LedReadout } from "../ui/LedReadout";
import { StatusPopover, type StatusPopoverProps } from "./StatusPopover";

export type LampMode = "wait" | "ok" | "warn" | "live";

export function lampState(
  connection: Connection,
  activeOutput: ActiveOutput | null,
): { mode: LampMode; label: string } {
  switch (connection) {
    case "connecting":
      return { mode: "wait", label: "connecting" };
    case "unreachable":
      return { mode: "warn", label: "core unreachable" };
    case "paused":
      return { mode: "warn", label: "service paused" };
    case "ok":
      return activeOutput ? { mode: "live", label: "ok, on air" } : { mode: "ok", label: "ok" };
  }
}

const FACE: Record<LampMode, string> = {
  wait: "wordmark-wait-face",
  ok: "wordmark-ok-face",
  warn: "wordmark-warn-face",
  live: "wordmark-live-face",
};

export interface HeaderProps extends Omit<StatusPopoverProps, "paused" | "status"> {
  connection: Connection;
  liveUpdates: ConnectionState;
  status: StatusResponse | null;
  activeOutput: ActiveOutput | null;
  pin: string | null;
}

export function Header({
  connection,
  liveUpdates,
  status,
  activeOutput,
  pin,
  ...popover
}: HeaderProps) {
  const lamp = lampState(connection, activeOutput);
  const address = status?.lan_addresses?.[0];
  const allAddresses = status?.lan_addresses?.map((a) => `${a}:${DEFAULT_PORT}`).join(", ");
  const liveOff = liveUpdates === "reconnecting" && connection === "ok";

  return (
    <header className="flex flex-wrap items-center justify-between gap-2.5 gap-x-[18px] border-b border-line-soft px-4 py-3 max-[720px]:p-3">
      <div className="flex items-center gap-3.5">
        <h1
          className={`wordmark relative m-0 inline-flex items-center rounded-sm px-3 py-1.5 text-lg font-normal [@media(max-height:560px)]:px-2.5 [@media(max-height:560px)]:py-1 ${lamp.mode === "live" ? "wordmark-live" : ""}`}
          title={lamp.label}
          data-testid="core-status"
        >
          <span
            className={`wordmark-gel pointer-events-none absolute inset-[3px] z-1 rounded-sm ${lamp.mode === "live" ? "opacity-70" : "opacity-55"}`}
            aria-hidden="true"
          />
          <span className={`relative z-2 ${FACE[lamp.mode]}`}>ONAIR</span>
          {/*
           * Polite live region rather than role="status": the device-help
           * panel owns the page's one status role, and the lamp colour alone
           * must not be the only signal.
           */}
          <span className="sr-only" aria-live="polite" aria-atomic="true">
            {`Service ${lamp.label}`}
          </span>
        </h1>
        {liveOff && (
          <span
            className="font-mono text-xs tracking-[0.12em] text-steel-dim uppercase"
            title="Live updates are reconnecting; the window refreshes every 15 seconds meanwhile."
            data-testid="live-updates-off"
          >
            live updates off
          </span>
        )}
      </div>
      <div className="flex flex-wrap items-center gap-2">
        <div className="flex items-center gap-1.5 rounded border border-line bg-linear-to-b from-face-top via-face to-face-deep px-2 py-1 shadow-[inset_0_1px_0_var(--color-line-strong),0_1px_2px_var(--color-black)]">
          <span className="font-mono text-xs tracking-[0.18em] text-steel-dim uppercase">pin</span>
          <LedReadout ghost="888888" testId="pairing-pin">
            {pin || "····"}
          </LedReadout>
          {address && (
            <>
              <span className="ml-1.5 font-mono text-xs tracking-[0.18em] text-steel-dim uppercase">
                addr
              </span>
              <span
                className="font-mono text-sm text-amber"
                data-testid="lan-address"
                title={allAddresses}
              >
                {address}
              </span>
            </>
          )}
        </div>
        <StatusPopover {...popover} status={status} paused={connection === "paused"} />
      </div>
    </header>
  );
}

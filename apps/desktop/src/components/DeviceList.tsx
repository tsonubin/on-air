import type {
  ActiveOutputView,
  AirPlayMode,
  OutputInfo,
  OutputSyncStatus,
} from "@on-air/api-types";
import { outputPhase, TRANSPORT_LABELS } from "@on-air/api-types";
import {
  inputLabels,
  SYNC_DRIFT_DESCRIPTION,
  SYNC_STATE_LABELS,
  SYNC_TRIM_DESCRIPTION,
  SYNC_TRIM_LIMIT_MS,
} from "@on-air/control-client";
import { useEffect, useMemo, useState } from "react";
import { isPickerOnly, outputKey } from "../hooks/useOutputSelection";
import { Button } from "../ui/Button";

const LIST_CLASS =
  "m-0 flex min-h-0 flex-1 list-none flex-col gap-1.5 overflow-x-hidden overflow-y-auto overscroll-contain p-0 pr-0.5";
const ROW_CLASS =
  "device-row grid w-full cursor-pointer grid-cols-[18px_minmax(0,1fr)_auto] items-center gap-2 rounded-md border border-line-soft px-2.5 py-2 text-left hover:border-line-strong focus-visible:outline-2 focus-visible:outline-amber disabled:cursor-not-allowed disabled:opacity-50";
const TAG_CLASS =
  "flex items-center gap-1.5 font-mono text-xs tracking-[0.14em] text-steel-dim uppercase";

function Lamp({ on }: { on: boolean }) {
  return (
    <span
      aria-hidden="true"
      className={`size-[9px] rounded-full border ${on ? "border-live bg-live" : "border-steel-dim"}`}
    />
  );
}

function Skeleton() {
  return (
    <>
      {[0, 1, 2].map((i) => (
        <li key={i} className="skeleton-row" aria-hidden="true" />
      ))}
      <li className="sr-only">Loading…</li>
    </>
  );
}

const SYNC_STATE_CLASS: Record<OutputSyncStatus["state"], string> = {
  free_running: "text-steel-dim",
  syncing: "text-amber",
  locked: "text-live",
};

/**
 * One output's sync standing and its manual latency trim. The trim is a
 * correction typed by ear: the copy never calls it calibrated.
 */
function SyncRow({
  status,
  disabled,
  onSetTrim,
}: {
  status: OutputSyncStatus;
  disabled: boolean;
  onSetTrim(trimMs: number): Promise<void>;
}) {
  const [draft, setDraft] = useState(String(status.trim_ms));
  const [busy, setBusy] = useState(false);
  useEffect(() => {
    setDraft(String(status.trim_ms));
  }, [status.trim_ms]);
  const trimmed = draft.trim();
  const parsed = Number(trimmed);
  const valid =
    trimmed !== "" &&
    Number.isInteger(parsed) &&
    Math.abs(parsed) <= SYNC_TRIM_LIMIT_MS &&
    parsed !== status.trim_ms;
  const drift = status.drift_ppm === undefined ? "" : ` · ${Math.round(status.drift_ppm)} ppm`;
  return (
    <div className="device-sync" data-testid={`sync-${status.transport}-${status.device_id}`}>
      <span className={SYNC_STATE_CLASS[status.state]} title={SYNC_DRIFT_DESCRIPTION}>
        {SYNC_STATE_LABELS[status.state]}
        {drift}
      </span>
      <label title={SYNC_TRIM_DESCRIPTION}>
        Latency trim
        <input
          type="number"
          step={5}
          min={-SYNC_TRIM_LIMIT_MS}
          max={SYNC_TRIM_LIMIT_MS}
          value={draft}
          disabled={disabled || busy}
          aria-label={`Latency trim for ${status.device_name} in milliseconds`}
          onChange={(e) => setDraft(e.target.value)}
        />
        ms
      </label>
      <Button
        data-testid={`set-trim-${status.transport}-${status.device_id}`}
        disabled={disabled || busy || !valid}
        aria-busy={busy}
        onClick={() => {
          setBusy(true);
          void onSetTrim(parsed).finally(() => setBusy(false));
        }}
      >
        Set trim
      </Button>
    </div>
  );
}

export interface DeviceListProps {
  inputs: string[] | null;
  activeInput: string | null;
  outputs: OutputInfo[] | null;
  activeOutput: ActiveOutputView | null;
  airplayMode: AirPlayMode | null;
  /** Per-output sync status rows; `null` on an older core. */
  sync: OutputSyncStatus[] | null;
  connectingOutput: string | null;
  refreshing: boolean;
  disabled: boolean;
  onChooseInput(name: string): void;
  onChooseOutput(output: OutputInfo): void;
  /** Applies one output's manual latency trim in milliseconds. */
  onSetTrim(output: OutputInfo, trimMs: number): Promise<void>;
  onBluetoothSettings(): void;
  onAirplayInfo(): void;
  onRefresh(): void;
}

export function DeviceList({
  inputs,
  activeInput,
  outputs,
  activeOutput,
  airplayMode,
  sync,
  connectingOutput,
  refreshing,
  disabled,
  onChooseInput,
  onChooseOutput,
  onSetTrim,
  onBluetoothSettings,
  onAirplayInfo,
  onRefresh,
}: DeviceListProps) {
  const inputRows = useMemo(() => (inputs ? inputLabels(inputs) : null), [inputs]);
  const phase = activeOutput ? outputPhase(activeOutput) : null;

  return (
    <div className="routing-grid">
      <section className="routing-section" aria-labelledby="source-heading">
        <div className="routing-heading">
          <h2 id="source-heading">Source</h2>
        </div>
        <ul className={LIST_CLASS} data-testid="input-list" aria-busy={inputRows === null}>
          {inputRows === null && <Skeleton />}
          {inputRows?.length === 0 && (
            <li className="px-1 py-2.5 text-sm text-steel-dim">No capture devices</li>
          )}
          {inputRows?.map(({ name, label }) => {
            const on = activeInput === name;
            return (
              <li key={name}>
                <button
                  type="button"
                  className={`${ROW_CLASS} ${on ? "device-row-on" : ""}`}
                  data-testid={`input-${name}`}
                  aria-pressed={on}
                  title={label === name ? undefined : name}
                  disabled={disabled}
                  onClick={() => onChooseInput(name)}
                >
                  <Lamp on={on} />
                  <span className="truncate whitespace-nowrap">{label}</span>
                  <span className={TAG_CLASS}>{on ? "in" : ""}</span>
                </button>
              </li>
            );
          })}
        </ul>
      </section>

      <section
        className="routing-section routing-destination"
        aria-labelledby="destination-heading"
      >
        <div className="routing-heading">
          <h2 id="destination-heading">Destination</h2>
          <div className="device-actions">
            <Button data-testid="add-bluetooth" onClick={onBluetoothSettings} disabled={disabled}>
              Bluetooth settings
            </Button>
            {airplayMode === "avroute-picker" && (
              <Button onClick={onAirplayInfo}>AirPlay info</Button>
            )}
            <Button
              data-testid="refresh-devices"
              disabled={refreshing || disabled}
              aria-busy={refreshing}
              onClick={onRefresh}
            >
              {refreshing ? "Refreshing…" : "Refresh"}
            </Button>
          </div>
        </div>
        <p id="airplay-limitation" hidden>
          AirPlay playback is not available on this Mac. Select for details.
        </p>
        <p data-testid="active-output" hidden>
          {activeOutput
            ? `${activeOutput.transport}: ${activeOutput.device_name}${phase === "live" ? "" : ` (${phase})`}`
            : "none"}
        </p>
        <ul className={LIST_CLASS} data-testid="output-list" aria-busy={outputs === null}>
          {outputs === null && <Skeleton />}
          {outputs?.length === 0 && (
            <li className="px-1 py-2.5 text-sm text-steel-dim">
              No speakers found. Connect Bluetooth in settings, or keep network speakers on the same
              LAN.
            </li>
          )}
          {outputs?.map((output) => {
            const key = outputKey(output);
            const current =
              activeOutput?.transport === output.transport && activeOutput?.device_id === output.id;
            // Only a live output is "on"; a starting one shows as connecting
            // and a failed one stays selectable so it can be chosen again.
            const on = current && phase === "live";
            const failed = current && phase === "failed";
            const connecting = connectingOutput === key || (current && phase === "starting");
            const pair = output.member_count >= 2 || output.kind === "pair";
            const pickerOnly = isPickerOnly(output, airplayMode);
            const syncStatus =
              sync?.find((s) => s.transport === output.transport && s.device_id === output.id) ??
              null;
            return (
              <li key={key}>
                <button
                  type="button"
                  className={`${ROW_CLASS} ${on ? "device-row-on" : ""}`}
                  data-testid={`output-${key}`}
                  aria-pressed={on}
                  aria-busy={connecting}
                  onClick={() => onChooseOutput(output)}
                  disabled={disabled || connectingOutput !== null}
                  aria-describedby={pickerOnly ? "airplay-limitation" : undefined}
                >
                  <Lamp on={on} />
                  <span className="truncate whitespace-nowrap">
                    {output.name}
                    {connecting ? " · Connecting…" : ""}
                    {failed && !connecting && <span className="text-amber"> · Failed</span>}
                  </span>
                  <span className={TAG_CLASS}>
                    {pair && (
                      <span className="inline-flex gap-0.5" title="stereo pair">
                        <span className="sr-only">stereo pair, </span>
                        <i className="block h-2.5 w-1.5 rounded-[1px_3px_3px_1px] border border-amber" />
                        <i className="block h-2.5 w-1.5 rounded-[1px_3px_3px_1px] border border-amber" />
                      </span>
                    )}
                    {pickerOnly
                      ? "unavailable · info"
                      : output.needs_pair && !output.paired
                        ? "pin"
                        : TRANSPORT_LABELS[output.transport]}
                  </span>
                </button>
                {syncStatus && (
                  <SyncRow
                    status={syncStatus}
                    disabled={disabled}
                    onSetTrim={(trimMs) => onSetTrim(output, trimMs)}
                  />
                )}
              </li>
            );
          })}
        </ul>
      </section>
    </div>
  );
}

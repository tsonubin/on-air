import type {
  ActiveOutputView,
  AirPlayMode,
  OutputInfo,
  OutputSyncStatus,
  SyncGroupView,
} from "@on-air/api-types";
import { outputPhase, TRANSPORT_LABELS } from "@on-air/api-types";
import {
  inputLabels,
  SYNC_DRIFT_DESCRIPTION,
  SYNC_GROUP_DESCRIPTION,
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
  groups,
  disabled,
  onSetTrim,
  onSetGroup,
}: {
  status: OutputSyncStatus;
  /** Groups this output could play with; empty on a core without groups. */
  groups: SyncGroupView[];
  disabled: boolean;
  onSetTrim(trimMs: number): Promise<void>;
  onSetGroup(groupId: number | null): Promise<void>;
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
  const group = groups.find((g) => g.id === status.group_id) ?? null;
  return (
    <div className="device-sync" data-testid={`sync-${status.transport}-${status.device_id}`}>
      <span className={SYNC_STATE_CLASS[status.state]} title={SYNC_DRIFT_DESCRIPTION}>
        {SYNC_STATE_LABELS[status.state]}
        {drift}
        {group ? ` · in ${group.name}` : ""}
      </span>
      {(groups.length > 0 || group !== null) && (
        <select
          data-testid={`group-select-${status.transport}-${status.device_id}`}
          aria-label={`Sync group for ${status.device_name}`}
          title={SYNC_GROUP_DESCRIPTION}
          value={String(status.group_id ?? "")}
          disabled={disabled || busy}
          onChange={(e) => {
            const picked = e.target.value;
            setBusy(true);
            void onSetGroup(picked === "" ? null : Number(picked)).finally(() => setBusy(false));
          }}
        >
          <option value="">Playing alone</option>
          {groups.map((g) => (
            <option key={g.id} value={g.id}>
              {g.name}
            </option>
          ))}
        </select>
      )}
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

/**
 * One sync group: SEVERAL SPEAKERS PLAYING IN SYNC, never one merged device.
 * Start begins every member together, stop halts them together, dissolve
 * forgets the group.
 */
function SyncGroupRow({
  group,
  outputs,
  disabled,
  onStart,
  onStop,
  onDissolve,
}: {
  group: SyncGroupView;
  outputs: OutputInfo[] | null;
  disabled: boolean;
  onStart(groupId: number): Promise<void>;
  onStop(groupId: number): Promise<void>;
  onDissolve(groupId: number): Promise<void>;
}) {
  const [busy, setBusy] = useState<"start" | "stop" | "dissolve" | null>(null);
  return (
    <li className="sync-group" data-testid={`sync-group-${group.id}`}>
      <span className="sync-group-name">{group.name}</span>
      <span className="sync-group-members">
        {group.members.length === 0 && <span>No speakers yet</span>}
        {group.members.map((member) => (
          <span key={`${member.transport}-${member.device_id}`}>
            {member.device_name ||
              outputs?.find((o) => o.transport === member.transport && o.id === member.device_id)
                ?.name ||
              member.device_id}
            {member.error ? <span className="text-amber"> · {member.error}</span> : null}
          </span>
        ))}
      </span>
      <span className={TAG_CLASS}>
        <Lamp on={group.playing} />
        {group.playing ? "Playing together" : "Not playing"}
      </span>
      <div className="device-actions">
        <Button
          data-testid={`start-group-${group.id}`}
          disabled={disabled || busy !== null}
          aria-busy={busy === "start"}
          onClick={() => {
            setBusy("start");
            void onStart(group.id).finally(() => setBusy(null));
          }}
        >
          {busy === "start" ? "Starting…" : "Start together"}
        </Button>
        <Button
          data-testid={`stop-group-${group.id}`}
          disabled={disabled || busy !== null}
          onClick={() => {
            setBusy("stop");
            void onStop(group.id).finally(() => setBusy(null));
          }}
        >
          Stop
        </Button>
        <Button
          data-testid={`dissolve-group-${group.id}`}
          disabled={disabled || busy !== null}
          onClick={() => {
            setBusy("dissolve");
            void onDissolve(group.id).finally(() => setBusy(null));
          }}
        >
          Dissolve
        </Button>
      </div>
    </li>
  );
}

/** The "Play together" section: the group list plus the create form. */
function SyncGroupsPanel({
  groups,
  outputs,
  disabled,
  onCreateGroup,
  onStart,
  onStop,
  onDissolve,
}: {
  groups: SyncGroupView[];
  outputs: OutputInfo[] | null;
  disabled: boolean;
  onCreateGroup(name: string): Promise<boolean>;
  onStart(groupId: number): Promise<void>;
  onStop(groupId: number): Promise<void>;
  onDissolve(groupId: number): Promise<void>;
}) {
  const [draft, setDraft] = useState("");
  const [creating, setCreating] = useState(false);
  const name = draft.trim();
  return (
    <section className="routing-together" aria-labelledby="together-heading">
      <div className="routing-heading">
        <h2 id="together-heading">Play together</h2>
      </div>
      <p className="sync-group-blurb" data-testid="sync-group-description">
        {SYNC_GROUP_DESCRIPTION}
      </p>
      <ul className="sync-group-list" data-testid="sync-group-list">
        {groups.length === 0 && <li className="px-1 py-1 text-sm text-steel-dim">No groups yet</li>}
        {groups.map((group) => (
          <SyncGroupRow
            key={group.id}
            group={group}
            outputs={outputs}
            disabled={disabled}
            onStart={onStart}
            onStop={onStop}
            onDissolve={onDissolve}
          />
        ))}
      </ul>
      <form
        className="sync-group-create"
        onSubmit={(e) => {
          e.preventDefault();
          if (name === "") return;
          setCreating(true);
          void onCreateGroup(name).then((created) => {
            setCreating(false);
            if (created) setDraft("");
          });
        }}
      >
        <input
          type="text"
          value={draft}
          maxLength={64}
          placeholder="New group name"
          aria-label="New group name"
          disabled={disabled || creating}
          onChange={(e) => setDraft(e.target.value)}
        />
        <Button
          type="submit"
          data-testid="create-group"
          disabled={disabled || creating || name === ""}
          aria-busy={creating}
        >
          {creating ? "Creating…" : "Create group"}
        </Button>
      </form>
    </section>
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
  /** Sync groups to play together; `[]` on a core without groups. */
  syncGroups: SyncGroupView[];
  connectingOutput: string | null;
  refreshing: boolean;
  disabled: boolean;
  onChooseInput(name: string): void;
  onChooseOutput(output: OutputInfo): void;
  /** Applies one output's manual latency trim in milliseconds. */
  onSetTrim(output: OutputInfo, trimMs: number): Promise<void>;
  /** Moves one output to play with `groupId`, or alone when `null`. */
  onSetSyncGroup(output: OutputInfo, groupId: number | null): Promise<void>;
  onCreateGroup(name: string): Promise<boolean>;
  onStartGroup(groupId: number): Promise<void>;
  onStopGroup(groupId: number): Promise<void>;
  onDissolveGroup(groupId: number): Promise<void>;
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
  syncGroups,
  connectingOutput,
  refreshing,
  disabled,
  onChooseInput,
  onChooseOutput,
  onSetTrim,
  onSetSyncGroup,
  onCreateGroup,
  onStartGroup,
  onStopGroup,
  onDissolveGroup,
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
                    groups={syncGroups}
                    disabled={disabled}
                    onSetTrim={(trimMs) => onSetTrim(output, trimMs)}
                    onSetGroup={(groupId) => onSetSyncGroup(output, groupId)}
                  />
                )}
              </li>
            );
          })}
        </ul>
      </section>

      <SyncGroupsPanel
        groups={syncGroups}
        outputs={outputs}
        disabled={disabled}
        onCreateGroup={onCreateGroup}
        onStart={onStartGroup}
        onStop={onStopGroup}
        onDissolve={onDissolveGroup}
      />
    </div>
  );
}

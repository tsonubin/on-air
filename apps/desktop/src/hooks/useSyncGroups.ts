import type { OutputInfo, SyncGroupMemberRequest } from "@on-air/api-types";
import { useCallback } from "react";
import { type ClientLike, liveClient } from "../lib/client";
import type { CoreSnapshotHandle } from "./useCoreSnapshot";

export interface UseSyncGroupsOptions {
  core: Pick<CoreSnapshotHandle, "snapshot" | "refresh" | "reportError" | "clearError">;
  client?: ClientLike;
}

export interface SyncGroupsHandle {
  /**
   * Moves one output to play with `groupId`, or alone when `null`. A move is
   * leave-then-add: the core refuses to add an output that is still in
   * another group.
   */
  setMembership(output: OutputInfo, groupId: number | null): Promise<void>;
  /** Creates a group from a name; refuses names outside 1..=64 characters. */
  createGroup(name: string): Promise<boolean>;
  startGroup(groupId: number): Promise<void>;
  stopGroup(groupId: number): Promise<void>;
  dissolveGroup(groupId: number): Promise<void>;
}

/**
 * Sync-group mutations, one at a time, with the same contract as
 * `useOutputSelection`: a refused mutation reports the error and leaves the
 * snapshot alone — `SyncGroupChanged` events and the next poll carry the
 * truth in.
 */
export function useSyncGroups({
  core,
  client = liveClient,
}: UseSyncGroupsOptions): SyncGroupsHandle {
  const { snapshot, refresh, reportError, clearError } = core;

  const perform = useCallback(
    async (action: () => Promise<void>): Promise<boolean> => {
      clearError();
      try {
        await action();
        return true;
      } catch (err) {
        reportError(err);
        return false;
      }
    },
    [clearError, reportError],
  );

  const setMembership = useCallback(
    async (output: OutputInfo, groupId: number | null) => {
      const member: SyncGroupMemberRequest = {
        transport: output.transport,
        device_id: output.id,
      };
      const current = snapshot.sync?.outputs.find(
        (s) => s.transport === output.transport && s.device_id === output.id,
      )?.group_id;
      const target = groupId ?? undefined;
      // Re-picking the current group is a no-op: leaving and re-adding would
      // stop this member if it is playing with its group right now.
      if (current === target) return;
      await perform(async () => {
        if (current !== undefined) await client.leaveSyncGroupMember(current, member);
        if (target !== undefined) await client.addSyncGroupMember(target, member);
        await refresh("devices");
      });
    },
    [client, perform, refresh, snapshot.sync],
  );

  const createGroup = useCallback(
    async (name: string): Promise<boolean> => {
      const trimmed = name.trim();
      if (trimmed.length < 1 || trimmed.length > 64) return false;
      return perform(async () => {
        await client.createSyncGroup(trimmed);
        await refresh("devices");
      });
    },
    [client, perform, refresh],
  );

  const startGroup = useCallback(
    async (groupId: number) => {
      // Slow by design: the call resolves once every member has started at
      // the group's shared presentation epoch.
      await perform(async () => {
        await client.startSyncGroup(groupId);
        await refresh("devices");
      });
    },
    [client, perform, refresh],
  );

  const stopGroup = useCallback(
    async (groupId: number) => {
      await perform(async () => {
        await client.stopSyncGroup(groupId);
        await refresh("devices");
      });
    },
    [client, perform, refresh],
  );

  const dissolveGroup = useCallback(
    async (groupId: number) => {
      await perform(async () => {
        await client.dissolveSyncGroup(groupId);
        await refresh("devices");
      });
    },
    [client, perform, refresh],
  );

  return { setMembership, createGroup, startGroup, stopGroup, dissolveGroup };
}

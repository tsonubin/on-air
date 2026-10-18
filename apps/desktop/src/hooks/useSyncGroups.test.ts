import type { SyncResponse } from "@on-air/api-types";
import { HttpError } from "@on-air/control-client";
import { act, renderHook } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { downstairs, fakeClient, fakeCore, mockSyncStatus, sonos, upstairs } from "../test/fakes";
import type { CoreSnapshot } from "./useCoreSnapshot";
import { useSyncGroups } from "./useSyncGroups";

const MEMBER = { transport: "sonos", device_id: sonos.id };

function setup(sync: SyncResponse) {
  // Writes mutate the fake's store; hand it copies so the snapshot the hook
  // reads (and shared fixtures) stay exactly as the test set them up.
  const client = fakeClient(
    fakeCore({
      sync: {
        outputs: sync.outputs.map((o) => ({ ...o })),
        groups: sync.groups?.map((g) => ({ ...g, members: g.members.map((m) => ({ ...m })) })),
      },
    }),
  );
  const core = {
    snapshot: { sync } as CoreSnapshot,
    refresh: vi.fn(async () => {}),
    reportError: vi.fn(),
    clearError: vi.fn(),
  };
  const hook = renderHook(() => useSyncGroups({ core, client }));
  return { client, core, hook };
}

describe("useSyncGroups", () => {
  it("moves an output by leaving its old group before joining the new one", async () => {
    const { client, core, hook } = setup({
      outputs: [mockSyncStatus({ group_id: downstairs.id })],
      groups: [downstairs, upstairs],
    });
    await act(() => hook.result.current.setMembership(sonos, upstairs.id));
    expect(client.leaveSyncGroupMember).toHaveBeenCalledWith(downstairs.id, MEMBER);
    expect(client.addSyncGroupMember).toHaveBeenCalledWith(upstairs.id, MEMBER);
    expect(client.leaveSyncGroupMember.mock.invocationCallOrder[0]).toBeLessThan(
      client.addSyncGroupMember.mock.invocationCallOrder[0],
    );
    expect(core.refresh).toHaveBeenCalledWith("devices");
    expect(core.reportError).not.toHaveBeenCalled();
  });

  it("leaves without joining when the target is playing alone", async () => {
    const { client, core, hook } = setup({
      outputs: [mockSyncStatus({ group_id: downstairs.id })],
      groups: [downstairs, upstairs],
    });
    await act(() => hook.result.current.setMembership(sonos, null));
    expect(client.leaveSyncGroupMember).toHaveBeenCalledWith(downstairs.id, MEMBER);
    expect(client.addSyncGroupMember).not.toHaveBeenCalled();
    expect(core.refresh).toHaveBeenCalledWith("devices");
  });

  it("joins without leaving when the output plays alone", async () => {
    const { client, core, hook } = setup({
      outputs: [mockSyncStatus()],
      groups: [downstairs, upstairs],
    });
    await act(() => hook.result.current.setMembership(sonos, upstairs.id));
    expect(client.leaveSyncGroupMember).not.toHaveBeenCalled();
    expect(client.addSyncGroupMember).toHaveBeenCalledWith(upstairs.id, MEMBER);
    expect(core.refresh).toHaveBeenCalledWith("devices");
  });

  it("changes nothing when the picked group is already the current one", async () => {
    const { client, core, hook } = setup({
      outputs: [mockSyncStatus({ group_id: downstairs.id })],
      groups: [downstairs, upstairs],
    });
    await act(() => hook.result.current.setMembership(sonos, downstairs.id));
    expect(client.leaveSyncGroupMember).not.toHaveBeenCalled();
    expect(client.addSyncGroupMember).not.toHaveBeenCalled();
    expect(core.refresh).not.toHaveBeenCalled();
  });

  it("changes nothing when an output playing alone is set to play alone", async () => {
    const { client, core, hook } = setup({
      outputs: [mockSyncStatus()],
      groups: [downstairs, upstairs],
    });
    await act(() => hook.result.current.setMembership(sonos, null));
    expect(client.leaveSyncGroupMember).not.toHaveBeenCalled();
    expect(client.addSyncGroupMember).not.toHaveBeenCalled();
    expect(core.refresh).not.toHaveBeenCalled();
  });

  it("starts every member at one shared epoch and reads the result back", async () => {
    const { client, core, hook } = setup({ outputs: [], groups: [downstairs] });
    await act(() => hook.result.current.startGroup(downstairs.id));
    expect(client.startSyncGroup).toHaveBeenCalledWith(downstairs.id);
    expect(core.refresh).toHaveBeenCalledWith("devices");
    expect(core.reportError).not.toHaveBeenCalled();
  });

  it("stops a group together and refreshes", async () => {
    const { client, core, hook } = setup({ outputs: [], groups: [downstairs] });
    await act(() => hook.result.current.stopGroup(downstairs.id));
    expect(client.stopSyncGroup).toHaveBeenCalledWith(downstairs.id);
    expect(core.refresh).toHaveBeenCalledWith("devices");
  });

  it("dissolves a group and refreshes", async () => {
    const { client, core, hook } = setup({ outputs: [], groups: [downstairs] });
    await act(() => hook.result.current.dissolveGroup(downstairs.id));
    expect(client.dissolveSyncGroup).toHaveBeenCalledWith(downstairs.id);
    expect(core.refresh).toHaveBeenCalledWith("devices");
  });

  it("creates a group from a trimmed name and refreshes", async () => {
    const { client, core, hook } = setup({ outputs: [], groups: [] });
    const created = await act(() => hook.result.current.createGroup("  Kitchen  "));
    expect(created).toBe(true);
    expect(client.createSyncGroup).toHaveBeenCalledWith("Kitchen");
    expect(core.refresh).toHaveBeenCalledWith("devices");
    expect(core.reportError).not.toHaveBeenCalled();
  });

  it("refuses group names outside 1..=64 characters", async () => {
    const { client, core, hook } = setup({ outputs: [], groups: [] });
    const blank = await act(() => hook.result.current.createGroup("   "));
    expect(blank).toBe(false);
    const overlong = await act(() => hook.result.current.createGroup("x".repeat(65)));
    expect(overlong).toBe(false);
    expect(client.createSyncGroup).not.toHaveBeenCalled();
    expect(core.refresh).not.toHaveBeenCalled();
    expect(core.reportError).not.toHaveBeenCalled();
  });

  it("reports a refused move and leaves the snapshot for events to correct", async () => {
    const { client, core, hook } = setup({
      outputs: [mockSyncStatus({ group_id: downstairs.id })],
      groups: [downstairs, upstairs],
    });
    const err = new HttpError(
      "/api/sync/groups/2/members",
      409,
      JSON.stringify({ error: "output already in a group", code: "conflict" }),
    );
    client.addSyncGroupMember.mockRejectedValueOnce(err);
    await act(() => hook.result.current.setMembership(sonos, upstairs.id));
    expect(client.leaveSyncGroupMember).toHaveBeenCalledWith(downstairs.id, MEMBER);
    expect(core.reportError).toHaveBeenCalledWith(err);
    expect(core.refresh).not.toHaveBeenCalled();
  });
});

import type { CdAction } from "@on-air/api-types";
import { useCallback } from "react";
import { type ClientLike, liveClient } from "../lib/client";
import type { CoreSnapshotHandle } from "./useCoreSnapshot";

/** CD keys. The control response is the new disc state, so no refetch follows. */
export function useCdTransport(
  core: Pick<CoreSnapshotHandle, "snapshot" | "patch" | "reportError" | "clearError">,
  client: ClientLike = liveClient,
) {
  const { snapshot, patch, reportError, clearError } = core;
  const send = useCallback(
    async (action: CdAction) => {
      clearError();
      try {
        patch.cd(await client.controlCd(action));
      } catch (err) {
        reportError(err);
      }
    },
    [clearError, client, patch, reportError],
  );
  return {
    playPause: () => void send(snapshot.cd?.playing ? "pause" : "play"),
    prev: () => void send("prev"),
    next: () => void send("next"),
  };
}

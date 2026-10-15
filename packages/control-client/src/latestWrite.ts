/**
 * Serialises writes to one endpoint and keeps only the newest unsent value,
 * so a fader drag produces at most one request in flight plus one queued.
 *
 * When a write fails and a newer value is queued, the newer value is sent:
 * it supersedes the failed one. Only when nothing newer is pending does
 * `onError` fire, with the value the core last accepted (or the value shown
 * before the first edit), so the caller can roll its optimistic patch back.
 */
export interface LatestWriteQueue<T> {
  /**
   * Replaces any value not yet sent; sends when the in-flight write settles.
   * `shown` is the value on screen before this edit: when nothing has been
   * confirmed yet it becomes the rollback target.
   */
  push(value: T, shown?: T): void;
  /** Drops the pending value and silences errors from writes already in flight. */
  reset(): void;
  /** A write is in flight or queued; polled values for this key should wait. */
  sending(): boolean;
}

export function createLatestWriteQueue<T>(
  send: (value: T) => Promise<void>,
  onError?: (error: unknown, value: T, committed: T | undefined) => void,
): LatestWriteQueue<T> {
  let pending: { value: T } | null = null;
  let committed: { value: T } | null = null;
  let sending = false;
  let generation = 0;

  const drain = async () => {
    if (sending) return;
    sending = true;
    try {
      while (pending) {
        const { value } = pending;
        pending = null;
        const startedIn = generation;
        try {
          await send(value);
          if (startedIn === generation) committed = { value };
        } catch (error) {
          const stale = startedIn !== generation;
          if (!stale && pending === null) onError?.(error, value, committed?.value);
        }
      }
    } finally {
      sending = false;
    }
  };

  return {
    push(value, shown) {
      // A new burst starts from what is on screen, which may be newer (a
      // poll or event) than the last write this queue saw accepted.
      if (!sending && shown !== undefined) committed = { value: shown };
      pending = { value };
      void drain();
    },
    reset() {
      pending = null;
      committed = null;
      generation += 1;
    },
    sending: () => sending,
  };
}

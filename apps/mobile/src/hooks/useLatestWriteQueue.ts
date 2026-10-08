import { useCallback, useEffect, useRef } from "react";

export type LatestWriteQueue<T> = {
  /**
   * Replaces any value not yet sent; sends when the in-flight write settles.
   * `shown` is the value on screen before this edit: when nothing has been
   * confirmed yet it becomes the rollback target.
   */
  push(value: T, shown?: T): void;
  /** Drops the pending value and silences errors from writes already in flight. */
  reset(): void;
};

/**
 * Serialises writes to one endpoint and keeps only the newest unsent value, so
 * a fader drag produces at most one request in flight plus one queued.
 *
 * When a write fails and a newer value is queued, the newer value is sent: it
 * supersedes the failed one. Only when nothing newer is pending does
 * `onError` fire, with the value the desktop last accepted (or the value shown
 * before the first edit), so the caller can roll its optimistic patch back.
 */
export function useLatestWriteQueue<T>(
  send: (value: T) => Promise<void>,
  onError?: (error: unknown, value: T, committed: T | undefined) => void,
): LatestWriteQueue<T> {
  const sendRef = useRef(send);
  sendRef.current = send;
  const onErrorRef = useRef(onError);
  onErrorRef.current = onError;
  const pending = useRef<{ value: T } | null>(null);
  const committed = useRef<{ value: T } | null>(null);
  const sending = useRef(false);
  const generation = useRef(0);
  const mounted = useRef(true);

  useEffect(() => {
    mounted.current = true;
    return () => {
      mounted.current = false;
    };
  }, []);

  const drain = useCallback(async () => {
    if (sending.current) return;
    sending.current = true;
    try {
      while (pending.current) {
        const { value } = pending.current;
        pending.current = null;
        const startedIn = generation.current;
        try {
          await sendRef.current(value);
          if (startedIn === generation.current) committed.current = { value };
        } catch (error) {
          const stale = startedIn !== generation.current;
          if (mounted.current && !stale && pending.current === null) {
            onErrorRef.current?.(error, value, committed.current?.value);
          }
        }
      }
    } finally {
      sending.current = false;
    }
  }, []);

  const push = useCallback(
    (value: T, shown?: T) => {
      // A new burst starts from what is on screen, which may be newer (a
      // poll or event) than the last write this queue saw accepted.
      if (!sending.current && shown !== undefined) committed.current = { value: shown };
      pending.current = { value };
      void drain();
    },
    [drain],
  );

  const reset = useCallback(() => {
    pending.current = null;
    committed.current = null;
    generation.current += 1;
  }, []);

  return { push, reset };
}

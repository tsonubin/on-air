import { useCallback, useEffect, useRef } from "react";

export type LatestWriteQueue<T> = {
  /** Replaces any value not yet sent; sends when the in-flight write settles. */
  push(value: T): void;
  /** Drops the pending value and silences errors from writes already in flight. */
  reset(): void;
};

/**
 * Serialises writes to one endpoint and keeps only the newest unsent value, so
 * a fader drag produces at most one request in flight plus one queued.
 * `onError` fires only for the newest write; a failure that a later value
 * already superseded is dropped.
 */
export function useLatestWriteQueue<T>(
  send: (value: T) => Promise<void>,
  onError?: (error: unknown, value: T) => void,
): LatestWriteQueue<T> {
  const sendRef = useRef(send);
  sendRef.current = send;
  const onErrorRef = useRef(onError);
  onErrorRef.current = onError;
  const pending = useRef<{ value: T } | null>(null);
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
        } catch (error) {
          const stale = startedIn !== generation.current;
          if (mounted.current && !stale && pending.current === null) {
            onErrorRef.current?.(error, value);
          }
        }
      }
    } finally {
      sending.current = false;
    }
  }, []);

  const push = useCallback(
    (value: T) => {
      pending.current = { value };
      void drain();
    },
    [drain],
  );

  const reset = useCallback(() => {
    pending.current = null;
    generation.current += 1;
  }, []);

  return { push, reset };
}

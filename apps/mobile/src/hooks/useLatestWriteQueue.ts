import { createLatestWriteQueue, type LatestWriteQueue } from "@on-air/control-client";
import { useEffect, useRef } from "react";

export type { LatestWriteQueue } from "@on-air/control-client";

/**
 * A stable latest-write queue for one endpoint. The queue always calls the
 * most recent `send` and `onError`, and never reports after unmount.
 */
export function useLatestWriteQueue<T>(
  send: (value: T) => Promise<void>,
  onError?: (error: unknown, value: T, committed: T | undefined) => void,
): LatestWriteQueue<T> {
  const sendRef = useRef(send);
  sendRef.current = send;
  const onErrorRef = useRef(onError);
  onErrorRef.current = onError;
  const mounted = useRef(true);
  useEffect(() => {
    mounted.current = true;
    return () => {
      mounted.current = false;
    };
  }, []);
  const queue = useRef<LatestWriteQueue<T> | null>(null);
  if (queue.current === null) {
    queue.current = createLatestWriteQueue<T>(
      (value) => sendRef.current(value),
      (error, value, committed) => {
        if (mounted.current) onErrorRef.current?.(error, value, committed);
      },
    );
  }
  return queue.current;
}

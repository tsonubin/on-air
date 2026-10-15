/**
 * Coalescing, serialised refresh scheduler shared by the desktop and phone
 * snapshot hooks. Requests name the slices they want; a burst inside the
 * coalesce window becomes one run of the union, and requests that arrive
 * while a run is in flight become exactly one more run after it.
 */
export interface RefreshQueue<S> {
  /** Adds slices to the next run. `immediate` runs now unless a run is in flight. */
  schedule(slices: readonly S[], immediate?: boolean): void;
  /** `schedule(slices, true)` plus a promise that resolves once a run covering them finished. */
  refreshNow(slices: readonly S[]): Promise<void>;
  /** Drops pending slices and the coalesce timer, forgets the in-flight run and releases waiters. */
  clear(): void;
  inFlight(): boolean;
}

export function createRefreshQueue<S>(
  run: (slices: S[]) => Promise<void>,
  coalesceMs: number,
): RefreshQueue<S> {
  let inFlight: Promise<void> | null = null;
  const pending = new Set<S>();
  let waiters: Array<() => void> = [];
  let timer: ReturnType<typeof setTimeout> | null = null;

  const clearTimer = () => {
    if (timer !== null) clearTimeout(timer);
    timer = null;
  };

  const flush = () => {
    clearTimer();
    if (inFlight !== null || pending.size === 0) return;
    const slices = [...pending];
    pending.clear();
    const settle = waiters;
    waiters = [];
    const promise = run(slices)
      .catch(() => {
        /* `run` reports its own failures; a throw must not wedge the queue. */
      })
      .finally(() => {
        if (inFlight === promise) inFlight = null;
        for (const resolve of settle) resolve();
        if (pending.size > 0) flush();
      });
    inFlight = promise;
  };

  const schedule = (slices: readonly S[], immediate = false) => {
    for (const slice of slices) pending.add(slice);
    if (immediate) {
      flush();
      return;
    }
    if (timer === null) timer = setTimeout(flush, coalesceMs);
  };

  return {
    schedule,
    refreshNow(slices) {
      const done = new Promise<void>((resolve) => waiters.push(resolve));
      schedule(slices, true);
      return done;
    },
    clear() {
      clearTimer();
      pending.clear();
      inFlight = null;
      const settle = waiters;
      waiters = [];
      for (const resolve of settle) resolve();
    },
    inFlight: () => inFlight !== null,
  };
}

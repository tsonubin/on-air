import { useEffect, useRef, useState } from "react";
import { theme } from "@/theme";

/**
 * A fader's local draft: it previews immediately and commits after a short
 * pause. While the person is adjusting, and for a moment after the last
 * commit settles, incoming `value` props (from polling or events) are ignored
 * so the control never snaps back mid-gesture; the latest prop is applied
 * once they let go.
 */
export function useFaderDraft({
  value,
  min,
  max,
  step,
  onChange,
}: {
  value: number;
  min: number;
  max: number;
  step: number;
  onChange: (value: number) => void | Promise<void>;
}): { draft: number; preview(next: number): void } {
  const clamp = (next: number) => {
    const bounded = Math.min(max, Math.max(min, next));
    return Math.round(bounded / step) * step;
  };
  const [draft, setDraft] = useState(value);
  const interacting = useRef(false);
  const latestProp = useRef(value);
  latestProp.current = value;
  const commitTimer = useRef<ReturnType<typeof setTimeout> | null>(null);
  const settleTimer = useRef<ReturnType<typeof setTimeout> | null>(null);
  const onChangeRef = useRef(onChange);
  onChangeRef.current = onChange;

  useEffect(() => {
    if (!interacting.current) setDraft(value);
  }, [value]);

  useEffect(
    () => () => {
      if (commitTimer.current) clearTimeout(commitTimer.current);
      if (settleTimer.current) clearTimeout(settleTimer.current);
    },
    [],
  );

  const release = () => {
    if (settleTimer.current) clearTimeout(settleTimer.current);
    settleTimer.current = setTimeout(() => {
      settleTimer.current = null;
      interacting.current = false;
      setDraft(latestProp.current);
    }, theme.motion.interactionSettleMs);
  };

  const commit = async (next: number) => {
    commitTimer.current = null;
    try {
      await onChangeRef.current(next);
    } catch {
      // The owner reports write failures; the fader only needs to let go.
    }
    // A newer preview restarted the cycle; let that one release.
    if (commitTimer.current === null) release();
  };

  const preview = (nextValue: number) => {
    const next = clamp(nextValue);
    interacting.current = true;
    if (settleTimer.current) clearTimeout(settleTimer.current);
    settleTimer.current = null;
    setDraft(next);
    if (commitTimer.current) clearTimeout(commitTimer.current);
    commitTimer.current = setTimeout(() => void commit(next), theme.motion.controlCommitMs);
  };

  return { draft, preview };
}

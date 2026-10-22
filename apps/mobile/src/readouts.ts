/** A value as the LED shows it and as a screen reader says it. */
export type Readout = { text: string; spoken: string };

/** A trimmed decimal: at most one place, no trailing ".0". */
const oneDecimal = (n: number) => String(Math.round(n * 10) / 10);

/** The output sample rate for the format corner (`44.1K`); the pipeline is always stereo. */
export function formatRate(hz: number): Readout {
  if (!(hz > 0)) return { text: "--.-K", spoken: "sample rate unknown" };
  const khz = oneDecimal(hz / 1000);
  return { text: `${khz}K`, spoken: `${khz} kilohertz stereo` };
}

/** An EQ gain, signed on the LED (`+3.5`) and in words aloud ("plus 3.5 decibels"). */
export function formatDb(db: number): Readout {
  const size = oneDecimal(Math.abs(db));
  if (db > 0) return { text: `+${size}`, spoken: `plus ${size} decibels` };
  if (db < 0) return { text: `-${size}`, spoken: `minus ${size} decibels` };
  return { text: "0", spoken: "0 decibels" };
}

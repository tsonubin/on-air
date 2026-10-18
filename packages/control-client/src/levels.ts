/** The bottom of the meter scale, in dBFS. */
export const METER_FLOOR_DB = -60;

/**
 * One channel's linear level (`0.0..=1.0`) as a fraction of the meter
 * scale: 0 at `METER_FLOOR_DB`, 1 at full scale.
 */
export function levelFraction(linear: number): number {
  if (linear <= 0) return 0;
  const db = 20 * Math.log10(linear);
  return Math.min(1, Math.max(0, (db - METER_FLOOR_DB) / -METER_FLOOR_DB));
}

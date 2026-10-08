import { prettyInput } from "./prettyInput";

export interface InputLabel {
  /** Raw device name; the key and the `data-testid` suffix. */
  name: string;
  /** What the row shows. Unique across the list. */
  label: string;
}

/**
 * Dedupes capture devices by their real name (the core can list one device
 * twice through two backends) and keeps every distinct device visible: when
 * two different names shorten to the same pretty label, the later ones get a
 * numeric suffix instead of being dropped.
 */
export function inputLabels(names: readonly string[]): InputLabel[] {
  const seenNames = new Set<string>();
  const labelCount = new Map<string, number>();
  const rows: InputLabel[] = [];
  for (const name of names) {
    if (seenNames.has(name)) continue;
    seenNames.add(name);
    const base = prettyInput(name);
    const count = (labelCount.get(base) ?? 0) + 1;
    labelCount.set(base, count);
    rows.push({ name, label: count === 1 ? base : `${base} (${count})` });
  }
  return rows;
}

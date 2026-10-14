import { AUDIO_CD_INPUT } from "@on-air/api-types";

/**
 * Shortens raw capture-device names for an input picker. The patterns are
 * Linux ALSA/PipeWire spellings seen on development machines; anything
 * unrecognised is shown as is.
 */
export function prettyInput(name: string): string {
  if (name === AUDIO_CD_INPUT) return AUDIO_CD_INPUT;
  if (name.startsWith("Discard all samples")) return "Null device";
  if (name.includes("PipeWire Sound Server")) return "PipeWire";
  if (name.startsWith("Default ALSA")) return "Default";
  if (name.includes("analog") && name.endsWith(".monitor")) return "Analog monitor";
  if (name.endsWith(".monitor")) return "System monitor";
  if (name.includes("CS4208 Analog")) return "Built-in analog";
  if (name.includes("HDMI")) return name.replace("HDA Intel HDMI, ", "HDMI ");
  return name;
}

export interface InputLabel {
  /** Raw device name: the key, the value sent to the core, the test id suffix. */
  name: string;
  /** What the row shows. Unique across the list. */
  label: string;
}

/**
 * Dedupes capture devices by their real name (the core can list one device
 * twice through two backends) and keeps every distinct device selectable:
 * when two different names shorten to the same label, the later ones get a
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

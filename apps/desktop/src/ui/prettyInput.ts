import { AUDIO_CD_INPUT } from "@on-air/api-types";

/**
 * Shorten raw capture-device names for the input picker. The patterns are
 * Linux ALSA/PipeWire spellings seen on development machines; anything
 * unrecognised is shown as-is.
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

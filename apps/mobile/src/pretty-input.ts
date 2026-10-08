/** Short labels for capture device names the desktop reports. */
export function prettyInput(name: string): string {
  if (name === "Audio CD") return "Audio CD";
  if (name.startsWith("Discard all samples")) return "Null device";
  if (name.includes("PipeWire Sound Server")) return "PipeWire";
  if (name.startsWith("Default ALSA")) return "Default";
  if (name.includes("analog") && name.endsWith(".monitor")) return "Analog monitor";
  if (name.endsWith(".monitor")) return "System monitor";
  if (name.includes("CS4208 Analog")) return "Built-in analog";
  if (name.includes("HDMI")) return name.replace("HDA Intel HDMI, ", "HDMI ");
  return name;
}

/** Drops inputs whose pretty label duplicates an earlier one. */
export function uniqueByLabel(inputs: string[]): string[] {
  const seen = new Set<string>();
  return inputs.filter((name) => {
    const label = prettyInput(name);
    if (seen.has(label)) return false;
    seen.add(label);
    return true;
  });
}

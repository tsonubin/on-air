// Presentation helpers that do not belong in a transport client. Kept only so
// current importers (apps/mobile/src/controlClient.ts, desktop App.tsx) keep
// compiling until Phase 2 moves them to apps/desktop/src/ui/prettyInput.ts
// and Phase 3 deletes this file.

/**
 * @deprecated Desktop-only label mapping for Linux ALSA/PipeWire device
 * names. Moved to `apps/desktop/src/ui/prettyInput.ts`; removed in Phase 3.
 */
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

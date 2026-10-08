import { describe, expect, it } from "vitest";
import { inputLabels } from "./inputLabels";

describe("inputLabels", () => {
  it("drops exact duplicate names only", () => {
    expect(inputLabels(["Line In", "Line In", "Mic"])).toEqual([
      { name: "Line In", label: "Line In" },
      { name: "Mic", label: "Mic" },
    ]);
  });

  it("keeps distinct devices whose pretty labels collide, with a suffix", () => {
    const rows = inputLabels([
      "alsa_output.pci-0000_00_1f.3.analog-stereo.monitor",
      "alsa_output.usb-Scarlett.analog-stereo.monitor",
      "Mic",
    ]);
    expect(rows.map((r) => r.label)).toEqual(["Analog monitor", "Analog monitor (2)", "Mic"]);
    expect(rows.map((r) => r.name)).toHaveLength(3);
  });
});

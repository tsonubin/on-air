import { describe, expect, it } from "vitest";
import { prettyInput } from "./prettyInput";

describe("prettyInput", () => {
  it("shortens common capture device names", () => {
    expect(prettyInput("Discard all samples (playback)")).toBe("Null device");
    expect(prettyInput("alsa_output.pci.analog-stereo.monitor")).toBe("Analog monitor");
    expect(prettyInput("PipeWire Sound Server")).toBe("PipeWire");
  });

  it("leaves the Audio CD input as is", () => {
    expect(prettyInput("Audio CD")).toBe("Audio CD");
  });
});

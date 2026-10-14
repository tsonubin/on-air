import assert from "node:assert/strict";
import { test } from "node:test";
import { inputLabels, prettyInput } from "./inputLabels.ts";

test("prettyInput shortens common capture device names", () => {
  assert.equal(prettyInput("Discard all samples (playback)"), "Null device");
  assert.equal(prettyInput("alsa_output.pci.analog-stereo.monitor"), "Analog monitor");
  assert.equal(prettyInput("PipeWire Sound Server"), "PipeWire");
  assert.equal(prettyInput("Audio CD"), "Audio CD");
  assert.equal(prettyInput("Scarlett 2i2"), "Scarlett 2i2");
});

test("inputLabels drops exact duplicate names only", () => {
  assert.deepEqual(inputLabels(["Line In", "Line In", "Mic"]), [
    { name: "Line In", label: "Line In" },
    { name: "Mic", label: "Mic" },
  ]);
});

test("inputLabels keeps distinct devices whose labels collide, with a suffix", () => {
  const rows = inputLabels([
    "alsa_output.pci-0000_00_1f.3.analog-stereo.monitor",
    "alsa_output.usb-Scarlett.analog-stereo.monitor",
    "Mic",
  ]);
  assert.deepEqual(
    rows.map((r) => r.label),
    ["Analog monitor", "Analog monitor (2)", "Mic"],
  );
  assert.equal(rows.length, 3);
});

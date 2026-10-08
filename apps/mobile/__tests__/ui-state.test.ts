import { deriveUiState, describeUiState } from "@/ui-state";

test("derives one state from hydration, pairing, phase and casting", () => {
  const base = { hydrated: true, paired: true, phase: "connected" as const, output: null };
  expect(deriveUiState({ ...base, hydrated: false })).toBe("restoring");
  expect(deriveUiState({ ...base, paired: false })).toBe("pairing");
  expect(deriveUiState({ ...base, phase: "idle" })).toBe("connecting");
  expect(deriveUiState({ ...base, phase: "connecting" })).toBe("connecting");
  expect(deriveUiState({ ...base, phase: "reconnecting" })).toBe("reconnecting");
  expect(deriveUiState({ ...base, phase: "paused", output: "live" })).toBe("paused");
  expect(deriveUiState({ ...base, phase: "unauthorized" })).toBe("pairing");
  expect(deriveUiState(base)).toBe("ready");
  expect(deriveUiState({ ...base, output: "live" })).toBe("live");
});

test("an output that is still starting is not live, and a failed one is not casting", () => {
  const base = { hydrated: true, paired: true, phase: "connected" as const };
  expect(deriveUiState({ ...base, output: "starting" })).toBe("starting");
  expect(deriveUiState({ ...base, output: "failed" })).toBe("ready");
  expect(describeUiState("starting")).toMatchObject({
    statusText: "Connecting speaker",
    live: false,
    controlsEnabled: true,
    volumeEnabled: false,
  });
});

test("maps labels and enabled flags from the state in one place", () => {
  expect(describeUiState("live")).toMatchObject({
    statusText: "Live",
    live: true,
    controlsEnabled: true,
    volumeEnabled: true,
    soundEnabled: true,
  });
  expect(describeUiState("ready")).toMatchObject({ volumeEnabled: false, controlsEnabled: true });
  expect(describeUiState("paused")).toMatchObject({
    statusText: "Paused",
    connectionText: "Connected to desktop",
    controlsEnabled: false,
    soundEnabled: false,
    paused: true,
  });
  expect(describeUiState("reconnecting")).toMatchObject({
    statusText: "Reconnecting",
    connectionText: "Reconnecting…",
    controlsEnabled: false,
    soundEnabled: true,
  });
});

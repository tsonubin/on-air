import type { ReactElement } from "react";
import ReactTestRenderer, { type ReactTestInstance } from "react-test-renderer";
import {
  navigationTheme,
  screenHeaderOptions,
  stackScreens,
  titledScreen,
} from "@/navigation-theme";
import { SettingsForm } from "@/settings-form";
import { sheetProps } from "@/sheets/sheet-props";
import { FormText } from "@/ui/form-text";
import { NativeIcon } from "@/ui/native-icon";
import AudioFormatScreen from "../app/audio-format";
import ConnectionScreen from "../app/connection";
import EqualizerScreen from "../app/equalizer";

jest.mock("@/remote-session", () => ({
  useRemoteSession: () => ({
    error: null,
    warning: null,
    view: { paused: false, controlsEnabled: true, connectionText: "Connected to desktop" },
    gains: [0, 0, 0, 0, 0],
    changeBand: jest.fn(),
    applyEq: jest.fn(),
    pairedHost: "192.168.5.14",
    refreshing: false,
    refresh: jest.fn(),
    disconnect: jest.fn(),
    sampleRate: 44100,
    outputSampleRate: 44100,
    inputRates: [44100, 48000],
    outputRates: [44100, 48000],
    configuringRate: false,
    applyRate: jest.fn(),
    captureBufferMs: 100,
    audioPerformance: { report: null, busy: false, error: null, run: jest.fn(), apply: jest.fn() },
  }),
}));

function create(element: ReactElement) {
  let renderer!: ReactTestRenderer.ReactTestRenderer;
  ReactTestRenderer.act(() => {
    renderer = ReactTestRenderer.create(element);
  });
  return renderer;
}

const isHost = (n: ReactTestInstance, name: string) => (n.type as unknown) === name;
const hosts = (r: ReactTestRenderer.ReactTestRenderer, name: string) =>
  r.root.findAll((n) => isHost(n, name));

test("navigation theme uses the faceplate palette with amber controls", () => {
  expect(navigationTheme.dark).toBe(true);
  expect(navigationTheme.colors).toMatchObject({
    background: "#0a0908",
    card: "#1f1b16",
    text: "#f3ead4",
    border: "#2e2a24",
    primary: "#e0a24b",
    notification: "#ff3b2a",
  });
  expect(screenHeaderOptions).toMatchObject({
    headerTintColor: "#e0a24b",
    headerTitleStyle: { fontFamily: "IBMPlexMono-Medium", color: "#f3ead4" },
    headerShadowVisible: false,
  });
});

test("native header titles are uppercase captions; back buttons keep the plain title", () => {
  expect(titledScreen("Audio format")).toEqual({
    title: "Audio format",
    headerTitle: "AUDIO FORMAT",
  });
  // Every screen is pushed straight from the Mixer, whose plain title becomes
  // the back label; the Sound menu and its "Sound" back labels are gone.
  const byName = Object.fromEntries(stackScreens.map((s) => [s.name, s.options]));
  expect(Object.keys(byName)).toEqual(["equalizer", "audio-format", "connection"]);
  for (const options of Object.values(byName)) {
    expect(options).not.toHaveProperty("headerBackTitle");
  }
});

test("sheets are faceplate-backed and amber-tinted on iOS", () => {
  expect(sheetProps({ kind: "source" }).modifiers).toEqual(
    expect.arrayContaining([
      { $type: "presentationBackground", color: "#1a1814" },
      { $type: "tint", color: "#e0a24b" },
    ]),
  );
});

test("settings forms hide the system background and seed amber", () => {
  const r = create(<SettingsForm testID="f">{null}</SettingsForm>);
  expect(hosts(r, "ExpoHost")[0].props.seedColor).toBe("#e0a24b");
  expect(hosts(r, "ExpoFieldGroup")[0].props.modifiers).toEqual(
    expect.arrayContaining([
      { $type: "scrollContentBackground", visible: "hidden" },
      { $type: "background", color: "#0a0908" },
    ]),
  );
});

test("form text uses the vendored families and uppercases labels", () => {
  const label = hosts(create(<FormText variant="label">Tone</FormText>), "ExpoText")[0];
  expect(label.props.children).toBe("TONE");
  // Expo UI sizes a custom family at `size ?? 17`; textStyle only picks the Dynamic Type curve.
  expect(label.props.modifiers).toEqual([
    { $type: "font", family: "IBMPlexMono-Medium", textStyle: "footnote", size: 13 },
  ]);
  const footnote = hosts(create(<FormText variant="footnote">Note</FormText>), "ExpoText")[0];
  expect(footnote.props.modifiers).toEqual([
    { $type: "font", family: "IBMPlexSans", textStyle: "footnote", size: 13 },
  ]);
  const body = hosts(create(<FormText variant="body">Row</FormText>), "ExpoText")[0];
  expect(body.props.modifiers).toEqual([
    { $type: "font", family: "IBMPlexSans", textStyle: "body", size: 17 },
  ]);
  expect(label.props.textStyle).toMatchObject({ color: "#9a8f78" });

  const value = hosts(create(<FormText variant="value">Custom</FormText>), "ExpoText")[0];
  expect(value.props.children).toBe("Custom");
  expect(value.props.textStyle).toMatchObject({ color: "#e0a24b" });
});

test("settings sections sit on faceplate rows under mono section titles", () => {
  const r = create(<AudioFormatScreen />);
  const sections = hosts(r, "ExpoFieldSection");
  expect(sections.length).toBeGreaterThan(0);
  for (const section of sections) {
    expect(section.props.modifiers).toEqual([{ $type: "listRowBackground", color: "#1a1814" }]);
  }
  const headers = hosts(r, "ExpoFieldSectionHeader").map(
    (h) => h.findAll((n) => isHost(n, "ExpoText"))[0].props.children,
  );
  expect(headers).toEqual(["SAMPLE RATE", "AUDIO PERFORMANCE"]);
  // The caption must fill the header slot, not become a stray first row.
  const rowTexts = hosts(r, "ExpoFieldSectionRows").flatMap((rows) =>
    rows.findAll((n) => isHost(n, "ExpoText")).map((t) => t.props.children),
  );
  expect(rowTexts).not.toContain("SAMPLE RATE");
  expect(rowTexts).not.toContain("AUDIO PERFORMANCE");
});

test("every settings screen puts its section captions in the header slot", () => {
  for (const Screen of [EqualizerScreen, AudioFormatScreen, ConnectionScreen]) {
    const r = create(<Screen />);
    const sections = hosts(r, "ExpoFieldSection");
    const titled = sections.filter(
      (s) => s.findAll((n) => isHost(n, "ExpoFieldSectionHeader")).length,
    );
    expect(titled.length).toBeGreaterThan(0);
    for (const rows of hosts(r, "ExpoFieldSectionRows")) {
      const texts = rows.findAll((n) => isHost(n, "ExpoText")).map((t) => t.props.children);
      for (const t of texts) expect(typeof t === "string" && /^[A-Z ]+$/.test(t)).toBe(false);
    }
  }
});

test("decorative icons seed amber so Android never falls back to Material purple", () => {
  const host = hosts(create(<NativeIcon name="speaker" />), "ExpoHost")[0];
  expect(host.props.seedColor).toBe("#e0a24b");
});

import type { ReactElement } from "react";
import ReactTestRenderer, { type ReactTestInstance } from "react-test-renderer";
import { navigationTheme, screenHeaderOptions } from "@/navigation-theme";
import { SettingsForm } from "@/settings-form";
import { sheetProps } from "@/sheets/sheet-props";
import { FormText } from "@/ui/form-text";
import { NativeIcon } from "@/ui/native-icon";
import SoundScreen from "../app/sound";

jest.mock("@/remote-session", () => ({
  useRemoteSession: () => ({
    error: null,
    warning: null,
    view: { paused: false },
    gains: [0, 0, 0, 0, 0],
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
    card: "#1a1814",
    text: "#f3ead4",
    border: "#2e2a24",
    primary: "#e0a24b",
    notification: "#ff3b2a",
  });
  expect(screenHeaderOptions).toMatchObject({
    headerTintColor: "#e0a24b",
    headerTitleStyle: { fontFamily: "IBMPlexMono-Medium", color: "#f3ead4" },
  });
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
  expect(label.props.modifiers).toEqual([
    { $type: "font", family: "IBMPlexMono-Medium", textStyle: "footnote" },
  ]);
  expect(label.props.textStyle).toMatchObject({ color: "#9a8f78" });

  const value = hosts(create(<FormText variant="value">Custom</FormText>), "ExpoText")[0];
  expect(value.props.children).toBe("Custom");
  expect(value.props.textStyle).toMatchObject({ color: "#e0a24b" });
});

test("settings sections sit on faceplate rows under mono section titles", () => {
  const r = create(<SoundScreen />);
  const sections = hosts(r, "ExpoFieldSection");
  expect(sections.length).toBeGreaterThan(0);
  for (const section of sections) {
    expect(section.props.modifiers).toEqual([{ $type: "listRowBackground", color: "#1a1814" }]);
  }
  const headers = hosts(r, "ExpoFieldSectionHeader").map(
    (h) => h.findAll((n) => isHost(n, "ExpoText"))[0].props.children,
  );
  expect(headers).toEqual(["TONE", "ADVANCED"]);
});

test("decorative icons seed amber so Android never falls back to Material purple", () => {
  const host = hosts(create(<NativeIcon name="speaker" />), "ExpoHost")[0];
  expect(host.props.seedColor).toBe("#e0a24b");
});

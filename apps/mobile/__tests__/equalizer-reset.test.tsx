import { act, render } from "@testing-library/react-native";
import { Platform, StyleSheet } from "react-native";
import { useSafeAreaInsets } from "react-native-safe-area-context";
import EqualizerScreen from "../app/equalizer";

const mockSession = {
  gains: [4, 0, 0, 0, 0],
  view: { controlsEnabled: true },
  changeBand: jest.fn(),
  applyEq: jest.fn(),
};

jest.mock("@/remote-session", () => ({ useRemoteSession: () => mockSession }));

const original = Platform.OS;
const setOS = (os: string) => {
  (Platform as { OS: string }).OS = os;
};

afterEach(() => {
  setOS(original);
  mockSession.gains = [4, 0, 0, 0, 0];
  mockSession.view.controlsEnabled = true;
  mockSession.applyEq.mockClear();
});

const hostsOf = (view: ReturnType<typeof render>, name: string) =>
  view.UNSAFE_root.findAll((n) => (n.type as unknown) === name);

describe("on Android", () => {
  beforeEach(() => setOS("android"));

  test("the header has an amber RESET caption that flattens the EQ", () => {
    const view = render(<EqualizerScreen />);
    const reset = view.getByTestId("eq-reset");
    expect(reset.props.accessibilityState).toEqual({ disabled: false });
    expect(StyleSheet.flatten(view.getByText("Reset").props.style)).toMatchObject({
      fontFamily: "IBMPlexMono-Medium",
      textTransform: "uppercase",
      color: "#e0a24b",
    });
    // The toolbar button needs an image icon on Android, so it is not used there.
    expect(hostsOf(view, "NativeToolbar")).toHaveLength(0);

    act(() => reset.props.onPress());
    expect(mockSession.applyEq).toHaveBeenCalledWith([0, 0, 0, 0, 0]);
  });

  test("Reset is dimmed and inert while the EQ is already flat", () => {
    mockSession.gains = [0, 0, 0, 0, 0];
    const view = render(<EqualizerScreen />);
    const reset = view.getByTestId("eq-reset");
    expect(reset.props.accessibilityState).toEqual({ disabled: true });
    expect(StyleSheet.flatten(view.getByText("Reset").props.style).color).toBe("#9a8f78");
    // The mocked Pressable drops onPress while disabled, as the real one ignores presses.
    expect(reset.props.onPress).toBeUndefined();
  });

  test("Reset is inert while controls are disabled", () => {
    mockSession.view.controlsEnabled = false;
    const view = render(<EqualizerScreen />);
    const reset = view.getByTestId("eq-reset");
    expect(reset.props.accessibilityState).toEqual({ disabled: true });
    expect(reset.props.onPress).toBeUndefined();
  });
});

test("in landscape the faders keep clear of the sensor housing and home indicator", () => {
  // Found in review: only the bottom inset was used, so the scale sat under the notch.
  jest.mocked(useSafeAreaInsets).mockReturnValueOnce({ top: 0, right: 59, bottom: 21, left: 59 });
  const view = render(<EqualizerScreen />);
  const body = view.getByTestId("equalizer-body");
  expect(StyleSheet.flatten(body.props.style)).toMatchObject({
    paddingLeft: 59,
    paddingRight: 59,
    paddingBottom: 21,
  });
});

test("iOS keeps Reset in the native toolbar", () => {
  setOS("ios");
  const view = render(<EqualizerScreen />);
  expect(view.queryByTestId("eq-reset")).toBeNull();
  const [button] = hostsOf(view, "NativeToolbarButton");
  expect(button.props.children).toBe("Reset");
  expect(button.props.disabled).toBe(false);
  button.props.onPress();
  expect(mockSession.applyEq).toHaveBeenCalledWith([0, 0, 0, 0, 0]);
});

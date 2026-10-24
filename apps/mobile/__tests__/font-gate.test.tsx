import { useFonts } from "expo-font";
import { Text } from "react-native";
import ReactTestRenderer, { act } from "react-test-renderer";
import { fontAssets } from "@/font-assets";
import { FontGate } from "@/font-gate";
import { fonts } from "@/theme";
import RootLayout from "../app/_layout";

beforeEach(() => {
  jest
    .mocked(useFonts)
    .mockReset()
    .mockImplementation(() => [true, null]);
});

async function render() {
  let renderer!: ReactTestRenderer.ReactTestRenderer;
  await act(async () => {
    renderer = ReactTestRenderer.create(
      <FontGate>
        <Text testID="child">ready</Text>
      </FontGate>,
    );
  });
  return renderer;
}

test("loads every theme font under its PostScript name", async () => {
  await render();
  expect(Object.keys(fontAssets).sort()).toEqual(Object.values(fonts).sort());
  expect(useFonts).toHaveBeenCalledWith(fontAssets);
});

test("holds a well-colored view while fonts load", async () => {
  jest.mocked(useFonts).mockReturnValueOnce([false, null]);
  const renderer = await render();
  expect(renderer.root.findAllByProps({ testID: "child" })).toHaveLength(0);
  expect(renderer.root.findByProps({ testID: "font-gate" }).props.style).toMatchObject({
    flex: 1,
    backgroundColor: "#0a0908",
  });
});

test("renders once fonts load", async () => {
  const renderer = await render();
  expect(renderer.root.findAllByProps({ testID: "child" }).length).toBeGreaterThan(0);
});

test("the app root waits for fonts before mounting the remote", async () => {
  jest.mocked(useFonts).mockReturnValueOnce([false, null]);
  let renderer!: ReactTestRenderer.ReactTestRenderer;
  await act(async () => {
    renderer = ReactTestRenderer.create(<RootLayout />);
  });
  expect(renderer.root.findAllByProps({ testID: "font-gate" }).length).toBeGreaterThan(0);
  expect(renderer.root.findAllByProps({ testID: "pairing-screen" })).toHaveLength(0);
});

test("still renders, in system fonts, when loading fails", async () => {
  jest.mocked(useFonts).mockReturnValueOnce([false, new Error("bad font")]);
  const renderer = await render();
  expect(renderer.root.findAllByProps({ testID: "child" }).length).toBeGreaterThan(0);
  expect(renderer.root.findAllByProps({ testID: "font-gate" })).toHaveLength(0);
});

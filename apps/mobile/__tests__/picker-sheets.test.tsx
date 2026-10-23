import type { ReactElement } from "react";
import { Text } from "react-native";
import ReactTestRenderer, { type ReactTestInstance } from "react-test-renderer";
import { SheetPanel } from "@/sheets/sheet-panel";

function create(element: ReactElement) {
  let renderer!: ReactTestRenderer.ReactTestRenderer;
  ReactTestRenderer.act(() => {
    renderer = ReactTestRenderer.create(element);
  });
  return renderer;
}

const hostById = (r: ReactTestRenderer.ReactTestRenderer, id: string): ReactTestInstance =>
  r.root.find((n) => typeof n.type === "string" && n.props.testID === id);

test("a sheet panel is a full-bleed faceplate with a caption and Done", () => {
  const onClose = jest.fn();
  const r = create(
    <SheetPanel spec={{ kind: "source", count: 1 }} caption="Source" onClose={onClose} testID="p">
      <Text>Body</Text>
    </SheetPanel>,
  );
  const sheet = r.root.find((n) => (n.type as unknown) === "ExpoBottomSheet");
  expect(sheet.props).toMatchObject({
    isPresented: true,
    contentPadding: 0,
    containerColor: "#1f1b16",
    testID: "p",
  });
  sheet.props.onDismiss(); // swipe-down / Android back
  expect(onClose).toHaveBeenCalledTimes(1);
  expect(hostById(r, "p-caption").props).toMatchObject({
    accessibilityRole: "header",
    children: "Source",
  });
  hostById(r, "p-done").props.onPress();
  expect(onClose).toHaveBeenCalledTimes(2);
  expect(
    r.root.findAll((n) => n.props.testID === "faceplate-finish" && typeof n.type === "string"),
  ).toHaveLength(1);
});

test("the pair panel has no Done key", () => {
  const r = create(
    <SheetPanel
      spec={{ kind: "device-pair", transport: "airplay" }}
      caption="Pair speaker"
      done={false}
      onClose={jest.fn()}
      testID="p"
    >
      <Text>Body</Text>
    </SheetPanel>,
  );
  expect(r.root.findAll((n) => n.props.testID === "p-done")).toHaveLength(0);
});

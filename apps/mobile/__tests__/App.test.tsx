import React from "react";
import ReactTestRenderer from "react-test-renderer";
import App from "../App";

test("renders the remote chrome", async () => {
  let tree: ReactTestRenderer.ReactTestRenderer;
  await ReactTestRenderer.act(() => {
    tree = ReactTestRenderer.create(<App />);
  });
  const text = JSON.stringify(tree!.toJSON());
  expect(text).toContain("on-air remote");
  expect(text).toContain("ONAIR");
});

import { layout } from "@/theme";
import { isTwoPane } from "@/two-pane";

test("two panes from 600 × 500 pt: unfolded Duo, iPads, foldables", () => {
  expect(isTwoPane(599, 900)).toBe(false); // just under
  expect(isTwoPane(600, 900)).toBe(true);
  expect(isTwoPane(669, 951)).toBe(true); // Duo unfolded, upright
  expect(isTwoPane(951, 669)).toBe(true); // Duo unfolded, sideways
  expect(isTwoPane(900, 499)).toBe(false); // too short for the EQ
  expect(isTwoPane(844, 390)).toBe(false); // phone landscape keeps its layout
  expect(isTwoPane(375, 812)).toBe(false); // Duo folded / phones
  expect(isTwoPane(320, 900)).toBe(false); // narrow split view
});

test("the two-pane thresholds are layout tokens", () => {
  expect(layout).toMatchObject({ twoPaneMinWidth: 600, twoPaneMinHeight: 500 });
});

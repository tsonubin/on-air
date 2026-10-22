import { colors, fonts, radius, theme, type } from "@/theme";

test("palette matches the desktop tokens", () => {
  expect(colors).toMatchObject({
    background: "#0a0908",
    surface: "#1a1814",
    surfaceRaised: "#221f1a",
    surfacePressed: "#2a2218",
    iconSurface: "#2a261f",
    border: "#3a342a",
    borderStrong: "#4a4338",
    separator: "#2e2a24",
    label: "#f3ead4",
    secondaryLabel: "#c4b89a",
    tertiaryLabel: "#9a8f78",
    accent: "#ff3b2a",
    liveDim: "#3a332a",
    amber: "#e0a24b",
    amberBright: "#f0c56a",
    key: "#5c4a32",
    onAmber: "#141210",
    ledWell: "#070605",
    ledGhost: "#1c1812",
    signLitText: "#ff3b2a",
    signLitWell: "#1a0605",
    signLitLine: "#8a3a30",
    signOffText: "#4a4136",
    signOffWell: "#0e0c0a",
    selectedSurface: "#3a221c",
    errorSurface: "#3a1410",
    errorBorder: "#6a2a22",
    onError: "#f0b4ac",
    disabled: "#5b5246",
    onAccent: "#ffffff",
    faceTop: "#1f1b16",
    faceBottom: "#16130f",
    faceHighlight: "rgba(255,236,200,0.07)",
    faceRow: "rgba(42,38,31,0.55)",
    grooveDark: "#0d0b09",
    grooveLight: "rgba(255,236,200,0.06)",
    keyTop: "#2a261f",
    keyBottom: "#1d1a15",
    keyHighlight: "rgba(255,236,200,0.08)",
    capTop: "#8a8174",
    capBottom: "#4e473d",
  });
  expect(theme.seedColor).toBe(colors.amber);
});

test("radii are machined, not pillowy", () => {
  expect(radius).toEqual({ sm: 4, md: 6, lg: 8, xl: 10, xxl: 12 });
});

test("type presets use the vendored families", () => {
  expect(type.sign).toMatchObject({ fontFamily: fonts.sign, fontSize: 18, letterSpacing: 2 });
  expect(type.led.fontFamily).toBe(fonts.led);
  expect(type.label).toMatchObject({
    fontFamily: fonts.monoMedium,
    fontSize: 11,
    textTransform: "uppercase",
  });
  expect(type.label.letterSpacing).toBeCloseTo(11 * 0.16);
  expect(type.title.fontFamily).toBe(fonts.sansSemiBold);
  expect(type.body.fontFamily).toBe(fonts.sans);
  expect(theme.type).toBe(type);
});

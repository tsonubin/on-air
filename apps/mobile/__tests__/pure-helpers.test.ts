import type { OutputSyncStatus, SyncGroupView } from "@on-air/api-types";
import { DEFAULT_PORT } from "@on-air/api-types";
import { HttpError, inputLabels } from "@on-air/control-client";
import { Platform } from "react-native";
import { formatDesktopTarget, parseDesktopTarget, targetBase } from "@/desktop-target";
import { friendlyError } from "@/errors";
import { digitsOnly, isForeground } from "@/remote-session";
import { outputAction, syncSummary } from "@/sheets/output-sheet";
import { sheetProps } from "@/sheets/sheet-props";
import { groupSummary, membershipSummary } from "@/sheets/sync-groups";

describe("sheetProps", () => {
  const original = Platform.OS;
  afterEach(() => {
    (Platform as { OS: string }).OS = original;
  });

  test("iOS gets compact detents and an explicit dark presentation", () => {
    (Platform as { OS: string }).OS = "ios";
    const source = sheetProps({ kind: "source" });
    expect(source.snapPoints).toEqual([{ height: 280 }]);
    expect(source.showDragIndicator).toBe(true);
    expect(source.modifiers).toEqual([
      { $type: "environment", key: "colorScheme", value: "dark" },
      { $type: "presentationBackground", color: "#171719" },
      { $type: "tint", color: "#ff453a" },
    ]);
    expect(sheetProps({ kind: "output", count: 2 }).snapPoints).toEqual([{ height: 494 }]);
    expect(sheetProps({ kind: "output", count: 0 }).snapPoints).toEqual([{ height: 340 }]);
    expect(sheetProps({ kind: "output", count: 2, trim: true }).snapPoints).toEqual([
      { height: 644 },
    ]);
    expect(
      sheetProps({ kind: "output", count: 2, groups: { speakers: 2, groups: 1 } }).snapPoints,
    ).toEqual(["full"]);
    expect(
      sheetProps({ kind: "output", count: 2, groups: { speakers: 4, groups: 3 } }).snapPoints,
    ).toEqual(["full"]);
    expect(sheetProps({ kind: "output", count: 6 }).snapPoints).toEqual(["full"]);
    expect(sheetProps({ kind: "device-pair", transport: "airplay" }).snapPoints).toEqual([
      { height: 360 },
    ]);
    expect(sheetProps({ kind: "device-pair", transport: "bluetooth" }).snapPoints).toEqual([
      { height: 300 },
    ]);
  });

  test("Android keeps the Material defaults", () => {
    (Platform as { OS: string }).OS = "android";
    for (const spec of [
      { kind: "source" } as const,
      { kind: "output", count: 9 } as const,
      { kind: "device-pair", transport: "airplay" } as const,
    ]) {
      expect(sheetProps(spec)).toEqual({ showDragIndicator: true });
    }
  });
});

test("digitsOnly strips non-digits before truncating (PIN regex)", () => {
  expect(digitsOnly("12a34b56", 6)).toBe("123456");
  expect(digitsOnly(" 1-2 3\\D4", 6)).toBe("1234");
  expect(digitsOnly("123456789", 8)).toBe("12345678");
});

test("only background counts as inactive", () => {
  expect(isForeground("active")).toBe(true);
  expect(isForeground("inactive")).toBe(true);
  expect(isForeground("unknown")).toBe(true);
  expect(isForeground(null)).toBe(true);
  expect(isForeground("background")).toBe(false);
});

test("desktop targets keep their port end to end", () => {
  expect(parseDesktopTarget("")).toBeNull();
  expect(parseDesktopTarget(" 10.0.0.5 ")).toEqual({ host: "10.0.0.5", port: DEFAULT_PORT });
  expect(parseDesktopTarget("http://10.0.0.5:48000/")).toEqual({ host: "10.0.0.5", port: 48000 });
  expect(parseDesktopTarget("10.0.0.5:99999")).toEqual({
    host: "10.0.0.5:99999",
    port: DEFAULT_PORT,
  });
  expect(parseDesktopTarget("[fe80::1]:48000")).toEqual({ host: "[fe80::1]", port: 48000 });
  expect(parseDesktopTarget("10.0.0.5", 48123)).toEqual({ host: "10.0.0.5", port: 48123 });
  expect(targetBase({ host: "10.0.0.5", port: 48000 })).toBe("http://10.0.0.5:48000");
  expect(formatDesktopTarget({ host: "10.0.0.5", port: DEFAULT_PORT })).toBe("10.0.0.5");
  expect(formatDesktopTarget({ host: "10.0.0.5", port: 48000 })).toBe("10.0.0.5:48000");
});

test("error copy follows the envelope code, then the status", () => {
  const envelope = (status: number, code: string) =>
    new HttpError("/api/x", status, JSON.stringify({ error: "ignored text", code }));
  expect(friendlyError(envelope(401, "invalid_pin"))).toMatch(/code was not accepted/);
  expect(friendlyError(envelope(401, "not_paired"))).toMatch(/Pairing expired/);
  expect(friendlyError(envelope(429, "pin_lockout"))).toMatch(/Too many pairing attempts/);
  expect(friendlyError(envelope(503, "service_paused"))).toMatch(/service is paused/);
  expect(friendlyError(envelope(502, "transport_unreachable"))).toMatch(/speaker did not answer/);
  expect(friendlyError(new HttpError("/api/x", 404))).toMatch(/no longer available/);
  expect(friendlyError(envelope(409, "not_ready"))).toBe(
    "That speaker isn't ready. Check it's on and connected, then try again.",
  );
  expect(friendlyError(new Error("boom"), "do it")).toBe(
    "Could not do it. Check that both devices are on the same Wi-Fi and try again.",
  );
  expect(friendlyError(new TypeError("Network request failed"))).toMatch(
    /Could not reach the desktop/,
  );
  expect(friendlyError(envelope(409, "stream_busy"))).toMatch(/already playing/);
  // A rejected request is the one envelope whose message is written for the
  // listener: the core's own explanation is shown as it sent it.
  expect(
    friendlyError(
      new HttpError(
        "/api/outputs/channel-mode",
        400,
        JSON.stringify({ error: "Mono Speaker plays mono only.", code: "invalid_request" }),
      ),
    ),
  ).toBe("Mono Speaker plays mono only.");
});

test("inputs are labelled and every distinct device stays selectable", () => {
  expect(inputLabels(["a.monitor", "b.monitor", "PipeWire Sound Server"])).toEqual([
    { name: "a.monitor", label: "System monitor" },
    { name: "b.monitor", label: "System monitor (2)" },
    { name: "PipeWire Sound Server", label: "PipeWire" },
  ]);
});

test("a speaker row reads its phase: starting connects, failed offers a retry", () => {
  const output = {
    id: "s",
    name: "S",
    transport: "sonos" as const,
    kind: "solo" as const,
    member_count: 1,
    needs_pair: false,
    paired: true,
  };
  const base = { desktopOnly: false, working: false, output };
  expect(outputAction({ ...base, selected: true, phase: "live" })).toBe("Connected");
  expect(outputAction({ ...base, selected: true, phase: "starting" })).toBe("Connecting…");
  expect(outputAction({ ...base, selected: true, phase: "failed" })).toBe("Failed · Retry");
  expect(outputAction({ ...base, selected: false, phase: null })).toBe("Connect");
});

test("a sync line reads state, drift and the manual trim", () => {
  const row: OutputSyncStatus = {
    transport: "sonos",
    device_id: "s",
    device_name: "S",
    state: "locked",
    trim_ms: -100,
    default_latency_ms: 1000,
    effective_latency_ms: 900,
    calibrated: false,
    delay_frames: 0,
    output_sample_rate_hz: 48000,
    drift_ppm: 42.4,
  };
  expect(syncSummary(row)).toBe("Locked · 42 ppm · trim -100 ms");
  expect(syncSummary({ ...row, state: "free_running", drift_ppm: undefined, trim_ms: 0 })).toBe(
    "Free-running",
  );
});

test("a membership line names the group the speaker plays with", () => {
  const row: OutputSyncStatus = {
    transport: "sonos",
    device_id: "s",
    device_name: "S",
    state: "free_running",
    trim_ms: 0,
    default_latency_ms: 1000,
    effective_latency_ms: 1000,
    calibrated: false,
    delay_frames: 0,
    output_sample_rate_hz: 48000,
    group_id: 2,
  };
  const groups: SyncGroupView[] = [
    { id: 1, name: "Kitchen", members: [], playing: false },
    { id: 2, name: "Patio", members: [], playing: false },
  ];
  expect(membershipSummary(row, groups)).toBe("Playing with Patio");
  expect(membershipSummary({ ...row, group_id: undefined }, groups)).toBe("Playing alone");
  // The group dissolved under a stale row still reads as playing alone.
  expect(membershipSummary({ ...row, group_id: 9 }, groups)).toBe("Playing alone");
});

test("a group's card lists its speakers and whether they play together now", () => {
  const group: SyncGroupView = {
    id: 1,
    name: "Kitchen",
    members: [
      { transport: "sonos", device_id: "s", device_name: "Mock Sonos", playing: true },
      { transport: "airplay", device_id: "ap", device_name: "Living Room AirPlay", playing: true },
    ],
    playing: true,
  };
  expect(groupSummary(group)).toBe("Mock Sonos, Living Room AirPlay · playing together now");
  expect(groupSummary({ ...group, playing: false })).toBe(
    "Mock Sonos, Living Room AirPlay · not playing",
  );
  expect(groupSummary({ ...group, members: [], playing: false })).toBe(
    "No speakers yet · not playing",
  );
});

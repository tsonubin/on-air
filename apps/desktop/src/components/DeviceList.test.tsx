import type { OutputInfo } from "@on-air/api-types";
import { HttpError } from "@on-air/control-client";
import { act, render, screen, waitFor, within } from "@testing-library/react";
import { useCallback, useState } from "react";
import { describe, expect, it, vi } from "vitest";
import type { ActionError, CoreSnapshot } from "../hooks/useCoreSnapshot";
import { useOutputSelection } from "../hooks/useOutputSelection";
import { errorCopy } from "../lib/errorCopy";
import {
  type FakeClient,
  type FakeCore,
  fakeClient,
  fakeCore,
  homepod,
  sonos,
} from "../test/fakes";
import { DeviceList, type DeviceListProps } from "./DeviceList";
import { ErrorBanner } from "./ErrorBanner";

function props(overrides: Partial<DeviceListProps> = {}): DeviceListProps {
  return {
    inputs: [],
    activeInput: null,
    outputs: [],
    activeOutput: null,
    airplayMode: "owntone",
    connectingOutput: null,
    refreshing: false,
    disabled: false,
    onChooseInput: vi.fn(),
    onChooseOutput: vi.fn(),
    onSetChannelMode: vi.fn(),
    onBluetoothSettings: vi.fn(),
    onAirplayInfo: vi.fn(),
    onRefresh: vi.fn(),
    ...overrides,
  };
}

/**
 * DeviceList wired to the real selection hook and the app's error banner, so
 * channel-mode clicks travel the same path as the running window: client call,
 * devices refresh, `errorCopy` on failure.
 */
function ChannelModeHarness({ core, client }: { core: FakeCore; client: FakeClient }) {
  const [rows, setRows] = useState<OutputInfo[]>(() => core.outputs.map((o) => ({ ...o })));
  const [actionError, setActionError] = useState<ActionError | null>(null);
  const refresh = useCallback(async () => {
    setRows((await client.listOutputs()).map((o) => ({ ...o })));
  }, [client]);
  const reportError = useCallback((err: unknown) => {
    setActionError({ message: errorCopy(err), at: Date.now() });
  }, []);
  const clearError = useCallback(() => setActionError(null), []);
  const selection = useOutputSelection({
    core: {
      snapshot: { outputs: rows, airplayMode: "owntone" } as CoreSnapshot,
      refresh,
      reportError,
      clearError,
      patch: { cd: vi.fn(), activeOutput: vi.fn(), activeInput: vi.fn() },
    },
    client,
  });
  return (
    <>
      <ErrorBanner error={actionError} onDismiss={clearError} />
      <DeviceList
        {...props({
          outputs: rows,
          connectingOutput: selection.connectingOutput,
          onSetChannelMode: (output, mode) => void selection.setChannelMode(output, mode),
        })}
      />
    </>
  );
}

describe("DeviceList", () => {
  it("dedupes inputs by name and keeps colliding labels apart", () => {
    render(
      <DeviceList
        {...props({
          inputs: [
            "alsa_output.a.analog-stereo.monitor",
            "alsa_output.b.analog-stereo.monitor",
            "alsa_output.a.analog-stereo.monitor",
          ],
          activeInput: "alsa_output.b.analog-stereo.monitor",
        })}
      />,
    );
    const rows = within(screen.getByTestId("input-list")).getAllByRole("button");
    expect(rows.map((r) => r.textContent)).toEqual(["Analog monitor", "Analog monitor (2)in"]);
    expect(rows[0]).toHaveAttribute("aria-pressed", "false");
    expect(rows[1]).toHaveAttribute("aria-pressed", "true");
  });

  it("shows skeletons, not empty-state copy, before the first load", () => {
    render(<DeviceList {...props({ inputs: null, outputs: null })} />);
    expect(screen.queryByText(/no speakers found/i)).toBeNull();
    expect(screen.queryByText(/no capture devices/i)).toBeNull();
    expect(screen.getByTestId("output-list")).toHaveAttribute("aria-busy", "true");
  });

  it("marks the active output pressed and blocks rows while connecting", () => {
    render(
      <DeviceList
        {...props({
          outputs: [sonos, homepod],
          activeOutput: { transport: "sonos", device_id: sonos.id, device_name: sonos.name },
          connectingOutput: "airplay-hp-1",
        })}
      />,
    );
    const active = screen.getByTestId("output-sonos-uuid:mock-sonos");
    expect(active).toHaveAttribute("aria-pressed", "true");
    expect(active).toBeDisabled();
    expect(screen.getByTestId("output-airplay-hp-1")).toHaveTextContent("Connecting…");
    expect(screen.getByTestId("active-output")).toHaveTextContent("sonos: Mock Sonos");
  });

  it("shows a starting output as connecting, not as live", () => {
    render(
      <DeviceList
        {...props({
          outputs: [sonos, homepod],
          activeOutput: {
            transport: "sonos",
            device_id: sonos.id,
            device_name: sonos.name,
            state: "starting",
          },
        })}
      />,
    );
    const row = screen.getByTestId("output-sonos-uuid:mock-sonos");
    expect(row).toHaveAttribute("aria-pressed", "false");
    expect(row).toHaveAttribute("aria-busy", "true");
    expect(row).toHaveTextContent("Connecting…");
    expect(screen.getByTestId("active-output")).toHaveTextContent("sonos: Mock Sonos (starting)");
  });

  it("shows a failed output as an error that can be chosen again", () => {
    const onChooseOutput = vi.fn();
    render(
      <DeviceList
        {...props({
          outputs: [sonos, homepod],
          onChooseOutput,
          activeOutput: {
            transport: "sonos",
            device_id: sonos.id,
            device_name: sonos.name,
            state: "failed",
          },
        })}
      />,
    );
    const row = screen.getByTestId("output-sonos-uuid:mock-sonos");
    expect(row).toHaveAttribute("aria-pressed", "false");
    expect(row).toHaveTextContent("Failed");
    expect(row).toBeEnabled();
    row.click();
    expect(onChooseOutput).toHaveBeenCalledWith(sonos);
    expect(screen.getByTestId("active-output")).toHaveTextContent("sonos: Mock Sonos (failed)");
  });

  it("treats an output without a state (older core) as live", () => {
    render(
      <DeviceList
        {...props({
          outputs: [sonos],
          activeOutput: { transport: "sonos", device_id: sonos.id, device_name: sonos.name },
        })}
      />,
    );
    expect(screen.getByTestId("output-sonos-uuid:mock-sonos")).toHaveAttribute(
      "aria-pressed",
      "true",
    );
    expect(screen.getByTestId("active-output")).toHaveTextContent(/^sonos: Mock Sonos$/);
  });

  it("shows Auto when the core saved no channel mode and the saved label otherwise", () => {
    render(
      <DeviceList
        {...props({
          outputs: [
            sonos,
            { ...homepod, channel_mode: "stereo" },
            { ...sonos, id: "uuid:m", name: "Mono Speaker", channel_mode: "mono" },
          ],
        })}
      />,
    );
    expect(screen.getByTestId("channel-mode-sonos-uuid:mock-sonos")).toHaveTextContent("Auto");
    expect(screen.getByTestId("channel-mode-airplay-hp-1")).toHaveTextContent("Stereo");
    expect(screen.getByTestId("channel-mode-sonos-uuid:m")).toHaveTextContent("Mono");
  });

  it("explains the mode semantics in the badge and hints known-mono devices", () => {
    render(
      <DeviceList
        {...props({ outputs: [sonos, { ...homepod, channel_mode: "stereo", channels: 1 }] })}
      />,
    );
    const sonosChip = screen.getByTestId("channel-mode-sonos-uuid:mock-sonos");
    const podChip = screen.getByTestId("channel-mode-airplay-hp-1");
    expect(sonosChip.getAttribute("title")).toMatch(/fits the device/);
    expect(sonosChip.getAttribute("aria-label")).toMatch(/fits the device/);
    expect(podChip.getAttribute("title")).toMatch(/never downmixes/);
    expect(podChip.getAttribute("title")).toMatch(/plays mono only/);
  });

  it("cycles auto → stereo → mono through the client and shows the saved mode", async () => {
    const core = fakeCore({ outputs: [{ ...sonos }, { ...homepod }] });
    const client = fakeClient(core);
    render(<ChannelModeHarness core={core} client={client} />);
    const chip = () => screen.getByTestId("channel-mode-sonos-uuid:mock-sonos");

    expect(chip()).toHaveTextContent("Auto");
    await act(async () => chip().click());
    await waitFor(() =>
      expect(client.setChannelMode).toHaveBeenCalledWith({
        transport: "sonos",
        device_id: sonos.id,
        mode: "stereo",
      }),
    );
    await waitFor(() => expect(chip()).toHaveTextContent("Stereo"));

    await act(async () => chip().click());
    await waitFor(() => expect(chip()).toHaveTextContent("Mono"));
    expect(client.setChannelMode).toHaveBeenLastCalledWith({
      transport: "sonos",
      device_id: sonos.id,
      mode: "mono",
    });

    await act(async () => chip().click());
    await waitFor(() => expect(chip()).toHaveTextContent("Auto"));
    expect(client.setChannelMode).toHaveBeenLastCalledWith({
      transport: "sonos",
      device_id: sonos.id,
      mode: "auto",
    });
  });

  it("keeps the displayed mode and shows the core's explanation when a mode is refused", async () => {
    const explanation =
      "Mono Speaker plays mono only; stereo would drop the right channel. Choose Auto or Mono.";
    const err = new HttpError(
      "/api/outputs/channel-mode",
      400,
      JSON.stringify({ error: explanation, code: "invalid_request" }),
    );
    const core = fakeCore({ outputs: [{ ...sonos, name: "Mono Speaker", channels: 1 }] });
    const client = fakeClient(core);
    client.setChannelMode.mockRejectedValueOnce(err);
    render(<ChannelModeHarness core={core} client={client} />);
    const chip = () => screen.getByTestId("channel-mode-sonos-uuid:mock-sonos");

    expect(chip().getAttribute("title")).toMatch(/plays mono only/);
    // The attempt is made even though the device is known-mono…
    await act(async () => chip().click());
    await waitFor(() =>
      expect(client.setChannelMode).toHaveBeenCalledWith({
        transport: "sonos",
        device_id: sonos.id,
        mode: "stereo",
      }),
    );
    // …the refusal leaves the displayed mode alone and its words reach the
    // error banner.
    expect(chip()).toHaveTextContent("Auto");
    expect(screen.getByRole("alert")).toHaveTextContent(explanation);
  });
});

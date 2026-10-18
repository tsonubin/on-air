import { fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { homepod, sonos } from "../test/fakes";
import { DeviceList, type DeviceListProps } from "./DeviceList";

function props(overrides: Partial<DeviceListProps> = {}): DeviceListProps {
  return {
    inputs: [],
    activeInput: null,
    outputs: [],
    activeOutput: null,
    airplayMode: "owntone",
    sync: null,
    connectingOutput: null,
    refreshing: false,
    disabled: false,
    onChooseInput: vi.fn(),
    onChooseOutput: vi.fn(),
    onSetTrim: vi.fn(async () => {}),
    onBluetoothSettings: vi.fn(),
    onAirplayInfo: vi.fn(),
    onRefresh: vi.fn(),
    ...overrides,
  };
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

  it("shows the sync state with drift and sends a manual trim", async () => {
    const onSetTrim = vi.fn(async () => {});
    render(
      <DeviceList
        {...props({
          outputs: [sonos],
          onSetTrim,
          sync: [
            {
              transport: "sonos",
              device_id: sonos.id,
              device_name: sonos.name,
              state: "locked",
              trim_ms: -100,
              default_latency_ms: 1000,
              effective_latency_ms: 900,
              calibrated: false,
              delay_frames: 2400,
              output_sample_rate_hz: 48000,
              drift_ppm: 42.4,
            },
          ],
        })}
      />,
    );
    const row = screen.getByTestId(`sync-sonos-${sonos.id}`);
    expect(row).toHaveTextContent("Locked · 42 ppm");
    const input = within(row).getByRole("spinbutton");
    expect(input.closest("label")).toHaveAttribute(
      "title",
      expect.stringContaining("not measured or calibrated"),
    );
    expect(input).toHaveValue(-100);
    fireEvent.change(input, { target: { value: "-250" } });
    fireEvent.click(within(row).getByRole("button", { name: "Set trim" }));
    await waitFor(() => expect(onSetTrim).toHaveBeenCalledWith(sonos, -250));
  });

  it("hides the sync row on cores without sync status and reports free-running driftless", () => {
    const { rerender } = render(<DeviceList {...props({ outputs: [sonos], sync: null })} />);
    expect(screen.queryByTestId(`sync-sonos-${sonos.id}`)).toBeNull();
    rerender(
      <DeviceList
        {...props({
          outputs: [sonos],
          sync: [
            {
              transport: "sonos",
              device_id: sonos.id,
              device_name: sonos.name,
              state: "free_running",
              trim_ms: 0,
              default_latency_ms: 1000,
              effective_latency_ms: 1000,
              calibrated: false,
              delay_frames: 0,
              output_sample_rate_hz: 48000,
            },
          ],
        })}
      />,
    );
    const row = screen.getByTestId(`sync-sonos-${sonos.id}`);
    expect(row).toHaveTextContent(/^Free-running/);
    expect(row).not.toHaveTextContent("ppm");
  });
});

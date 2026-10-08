import { render, screen, within } from "@testing-library/react";
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
    connectingOutput: null,
    refreshing: false,
    disabled: false,
    onChooseInput: vi.fn(),
    onChooseOutput: vi.fn(),
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
    expect(screen.getByTestId("active-output")).toHaveTextContent("sonos: Kitchen");
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
    expect(screen.getByTestId("active-output")).toHaveTextContent("sonos: Kitchen (starting)");
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
    expect(screen.getByTestId("active-output")).toHaveTextContent("sonos: Kitchen (failed)");
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
    expect(screen.getByTestId("active-output")).toHaveTextContent(/^sonos: Kitchen$/);
  });
});

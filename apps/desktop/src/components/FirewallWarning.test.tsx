import type { FirewallWarning as Warning } from "@on-air/api-types";
import { repairFirewall } from "@on-air/control-client";
import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { beforeEach, expect, it, vi } from "vitest";
import { FirewallWarning } from "./FirewallWarning";

vi.mock("@on-air/control-client", () => ({ repairFirewall: vi.fn() }));
const warning: Warning = {
  subnet: "192.168.5.0/24",
  port: 47990,
  message: "UFW is blocking on-air audio from the local network.",
  repair_command: "sudo ufw allow from 192.168.5.0/24 to any port 47990 proto tcp",
};
beforeEach(() => vi.clearAllMocks());

it("requires an explicit action after explaining the rule and refreshes after repair", async () => {
  vi.mocked(repairFirewall).mockResolvedValue();
  const refreshed = vi.fn();
  render(<FirewallWarning warning={warning} onRepaired={refreshed} />);
  expect(repairFirewall).not.toHaveBeenCalled();
  fireEvent.click(screen.getByText("Firewall help"));
  expect(screen.getByText(/Allow devices on 192.168.5.0\/24/)).toBeInTheDocument();
  fireEvent.click(screen.getByRole("button", { name: "Allow LAN audio" }));
  await waitFor(() => expect(refreshed).toHaveBeenCalledOnce());
  expect(repairFirewall).toHaveBeenCalledOnce();
});

it("keeps manual instructions available when authorization fails", async () => {
  vi.mocked(repairFirewall).mockRejectedValue(new Error("cancelled"));
  const refreshed = vi.fn();
  render(<FirewallWarning warning={warning} onRepaired={refreshed} />);
  fireEvent.click(screen.getByText("Firewall help"));
  fireEvent.click(screen.getByRole("button", { name: "Allow LAN audio" }));
  await screen.findByText(/The rule was not applied/);
  expect(screen.getByText(warning.repair_command)).toBeInTheDocument();
  expect(refreshed).not.toHaveBeenCalled();
});

import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { useState } from "react";
import { describe, expect, it, vi } from "vitest";
import type { PairTarget } from "../hooks/useOutputSelection";
import { PairDialog } from "./PairDialog";

const airplay: PairTarget = { transport: "airplay", id: "hp-1", name: "Bedroom" };
const bluetooth: PairTarget = { transport: "bluetooth", id: "bt", name: "Headphones" };

function Harness({ target, onSubmit }: { target: PairTarget; onSubmit(pin: string): void }) {
  const [open, setOpen] = useState<PairTarget | null>(null);
  return (
    <>
      <button type="button" onClick={() => setOpen(target)}>
        Open pairing
      </button>
      <PairDialog
        target={open}
        pairing={false}
        onSubmit={onSubmit}
        onCancel={() => setOpen(null)}
      />
    </>
  );
}

describe("PairDialog", () => {
  it("is a labelled modal that focuses the PIN field", async () => {
    render(<PairDialog target={airplay} pairing={false} onSubmit={vi.fn()} onCancel={vi.fn()} />);
    const dialog = await screen.findByRole("dialog", { name: "Pair Bedroom" });
    expect(dialog).toHaveAttribute("aria-modal", "true");
    const pin = screen.getByLabelText(/speaker pin/i);
    expect(pin).toHaveAttribute("autocomplete", "one-time-code");
    await waitFor(() => expect(pin).toHaveFocus());
  });

  it("focuses the primary action when no PIN is needed", async () => {
    render(<PairDialog target={bluetooth} pairing={false} onSubmit={vi.fn()} onCancel={vi.fn()} />);
    await waitFor(() =>
      expect(screen.getByRole("button", { name: "Pair & go live" })).toHaveFocus(),
    );
  });

  it("submits the PIN on Enter", async () => {
    const user = userEvent.setup();
    const onSubmit = vi.fn();
    render(<PairDialog target={airplay} pairing={false} onSubmit={onSubmit} onCancel={vi.fn()} />);
    const pin = await screen.findByLabelText(/speaker pin/i);
    await user.type(pin, " 4321{Enter}");
    expect(onSubmit).toHaveBeenCalledWith("4321");
  });

  it("closes on Escape and returns focus to the opener", async () => {
    const user = userEvent.setup();
    render(<Harness target={airplay} onSubmit={vi.fn()} />);
    const opener = screen.getByRole("button", { name: "Open pairing" });
    await user.click(opener);
    await screen.findByRole("dialog");
    await user.keyboard("{Escape}");
    await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
    expect(opener).toHaveFocus();
  });

  it("ignores Escape while pairing is in flight", async () => {
    const user = userEvent.setup();
    const onCancel = vi.fn();
    render(<PairDialog target={airplay} pairing onSubmit={vi.fn()} onCancel={onCancel} />);
    await screen.findByRole("dialog");
    await user.keyboard("{Escape}");
    expect(onCancel).not.toHaveBeenCalled();
    expect(screen.getByRole("dialog")).toBeInTheDocument();
  });
});

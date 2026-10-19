import { runAudioBenchmark, setCaptureBuffer } from "@on-air/control-client";
import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { beforeEach, expect, it, vi } from "vitest";
import { AudioPerformance } from "./AudioPerformance";

vi.mock("@on-air/control-client", async (importOriginal) => ({
  ...(await importOriginal<typeof import("@on-air/control-client")>()),
  runAudioBenchmark: vi.fn(),
  setCaptureBuffer: vi.fn(),
}));
beforeEach(() => vi.clearAllMocks());

it("keeps a recommendation separate from applying it", async () => {
  vi.mocked(runAudioBenchmark).mockResolvedValue({
    input_hz: 44100,
    output_hz: 44100,
    duration_ms: 2200,
    pipeline_p95_ms: 10,
    scheduler_p95_ms: 2,
    worst_case_ms: 25,
    recommended_buffer_ms: 100,
    recommended_quality: "balanced",
    within_buffer_budget: true,
  });
  vi.mocked(setCaptureBuffer).mockResolvedValue();
  const applied = vi.fn();
  render(<AudioPerformance bufferMs={0} disabled={false} onApplied={applied} />);
  fireEvent.click(screen.getByText("Audio performance"));
  fireEvent.click(screen.getByRole("button", { name: "Run audio check" }));
  await screen.findByText(/Recommended: 100 ms/);
  expect(setCaptureBuffer).not.toHaveBeenCalled();
  fireEvent.click(screen.getByRole("button", { name: "Apply recommendation (100 ms)" }));
  await waitFor(() => expect(applied).toHaveBeenCalledOnce());
  expect(setCaptureBuffer).toHaveBeenCalledWith(expect.any(String), 100);
});

it("allows manual buffering without a benchmark", async () => {
  vi.mocked(setCaptureBuffer).mockResolvedValue();
  render(<AudioPerformance bufferMs={50} disabled={false} onApplied={vi.fn()} />);
  fireEvent.click(screen.getByText("Audio performance"));
  fireEvent.change(screen.getByLabelText("Capture buffer"), { target: { value: "200" } });
  expect(setCaptureBuffer).not.toHaveBeenCalled();
  fireEvent.click(screen.getByRole("button", { name: "Apply buffer" }));
  await waitFor(() => expect(setCaptureBuffer).toHaveBeenCalledWith(expect.any(String), 200));
  expect(runAudioBenchmark).not.toHaveBeenCalled();
});

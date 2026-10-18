import type * as ControlClient from "@on-air/control-client";
import { runAudioBenchmark, setAudioQuality, setCaptureBuffer } from "@on-air/control-client";
import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { beforeEach, expect, it, vi } from "vitest";
import { AudioPerformance } from "./AudioPerformance";

vi.mock("@on-air/control-client", async (importOriginal) => ({
  ...(await importOriginal<typeof ControlClient>()),
  runAudioBenchmark: vi.fn(),
  setCaptureBuffer: vi.fn(),
  setAudioQuality: vi.fn(),
}));
beforeEach(() => vi.clearAllMocks());

const QUALITY = {
  quality: "balanced" as const,
  recommended: "high" as const,
  profiles: ["economy" as const, "balanced" as const, "high" as const],
};

it("keeps a recommendation separate from applying it", async () => {
  vi.mocked(runAudioBenchmark).mockResolvedValue({
    input_hz: 44100,
    output_hz: 44100,
    duration_ms: 2200,
    pipeline_p95_ms: 10,
    scheduler_p95_ms: 2,
    worst_case_ms: 25,
    recommended_buffer_ms: 100,
    within_buffer_budget: true,
    recommended_quality: "high",
  });
  vi.mocked(setCaptureBuffer).mockResolvedValue();
  const applied = vi.fn();
  render(<AudioPerformance bufferMs={0} quality={QUALITY} disabled={false} onApplied={applied} />);
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
  render(<AudioPerformance bufferMs={50} quality={QUALITY} disabled={false} onApplied={vi.fn()} />);
  fireEvent.click(screen.getByText("Audio performance"));
  fireEvent.change(screen.getByLabelText("Capture buffer"), { target: { value: "200" } });
  expect(setCaptureBuffer).not.toHaveBeenCalled();
  fireEvent.click(screen.getByRole("button", { name: "Apply buffer" }));
  await waitFor(() => expect(setCaptureBuffer).toHaveBeenCalledWith(expect.any(String), 200));
  expect(runAudioBenchmark).not.toHaveBeenCalled();
});

it("shows the DSP quality profiles and applies a manual pick", async () => {
  vi.mocked(setAudioQuality).mockResolvedValue();
  const applied = vi.fn();
  render(<AudioPerformance bufferMs={0} quality={QUALITY} disabled={false} onApplied={applied} />);
  fireEvent.click(screen.getByText("Audio performance"));
  const select = screen.getByLabelText("DSP quality");
  expect(select).toHaveValue("balanced");
  expect(screen.getByText(/128-tap sinc resampler/)).toBeInTheDocument();
  expect(setAudioQuality).not.toHaveBeenCalled();
  fireEvent.change(select, { target: { value: "high" } });
  expect(screen.getByText(/256-tap sinc resampler/)).toBeInTheDocument();
  fireEvent.click(screen.getByRole("button", { name: "Apply quality" }));
  await waitFor(() => expect(setAudioQuality).toHaveBeenCalledWith(expect.any(String), "high"));
  await waitFor(() => expect(applied).toHaveBeenCalledOnce());
});

it("offers the check's recommended quality the way it offers the buffer", async () => {
  vi.mocked(runAudioBenchmark).mockResolvedValue({
    input_hz: 44100,
    output_hz: 44100,
    duration_ms: 2200,
    pipeline_p95_ms: 10,
    scheduler_p95_ms: 2,
    worst_case_ms: 25,
    recommended_buffer_ms: 100,
    within_buffer_budget: true,
    recommended_quality: "high",
  });
  vi.mocked(setAudioQuality).mockResolvedValue();
  render(<AudioPerformance bufferMs={0} quality={QUALITY} disabled={false} onApplied={vi.fn()} />);
  fireEvent.click(screen.getByText("Audio performance"));
  fireEvent.click(screen.getByRole("button", { name: "Run audio check" }));
  await screen.findByRole("button", { name: "Apply recommended quality (High)" });
  expect(setAudioQuality).not.toHaveBeenCalled();
  fireEvent.click(screen.getByRole("button", { name: "Apply recommended quality (High)" }));
  await waitFor(() => expect(setAudioQuality).toHaveBeenCalledWith(expect.any(String), "high"));
});

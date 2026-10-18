import type { AudioBenchmark, DspQuality } from "@on-air/api-types";

export const CAPTURE_BUFFER_OPTIONS = [0, 50, 100, 200, 400] as const;
export const captureBufferLabel = (ms: number) => (ms === 0 ? "System default" : `${ms} ms`);

export function audioBenchmarkSummary(report: AudioBenchmark): string {
  return `At ${report.input_hz / 1000} → ${report.output_hz / 1000} kHz: Recommended: ${report.recommended_buffer_ms} ms. Pipeline p95 ${report.pipeline_p95_ms.toFixed(1)} ms; scheduling p95 ${report.scheduler_p95_ms.toFixed(1)} ms. Includes 3× observed timing margin plus one audio quantum.`;
}

export const AUDIO_CHECK_DESCRIPTION =
  "Run during your usual workload. Uses an isolated DSP pipeline and leaves playback and system priority unchanged. Larger capture buffers tolerate scheduling stalls at the cost of more delay. Actual sizes depend on the audio driver. Hardware and network delays are not measured.";
export const AUDIO_CHECK_OVER_BUDGET =
  "Measured stalls exceed the available buffer budget. Reduce load or sample rate and run the check again.";

/** Selectable DSP quality profiles, cheapest first. */
export const DSP_QUALITY_OPTIONS: readonly DspQuality[] = ["economy", "balanced", "high"];

/** How each profile is spelled in pickers. */
export const DSP_QUALITY_LABELS: Record<DspQuality, string> = {
  economy: "Economy",
  balanced: "Balanced",
  high: "High",
};

/** What each profile costs: the resampler behind it and its CPU price. */
export const DSP_QUALITY_COSTS: Record<DspQuality, string> = {
  economy: "Cubic (Farrow) resampler: the cheapest, with rougher conversion.",
  balanced: "128-tap sinc resampler: clean conversion at moderate CPU cost.",
  high: "256-tap sinc resampler: the cleanest conversion, the highest CPU cost.",
};

export const DSP_QUALITY_DESCRIPTION =
  "DSP quality picks the resampler used between the input and output sample rates. It does not change the capture buffer or measured timing.";

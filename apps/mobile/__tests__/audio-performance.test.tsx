import { runAudioBenchmark, setCaptureBuffer } from "@on-air/control-client";
import { act, renderHook } from "@testing-library/react-native";
import { useAudioPerformance } from "../src/hooks/useAudioPerformance";

jest.mock("@on-air/control-client", () => ({
  runAudioBenchmark: jest.fn(),
  setCaptureBuffer: jest.fn(),
}));
const report = {
  input_hz: 44100,
  output_hz: 44100,
  duration_ms: 2200,
  pipeline_p95_ms: 10,
  scheduler_p95_ms: 2,
  worst_case_ms: 25,
  recommended_buffer_ms: 100,
  within_buffer_budget: true,
};
beforeEach(() => jest.clearAllMocks());

test("checking recommends without writing; applying explicitly updates the host", async () => {
  jest.mocked(runAudioBenchmark).mockResolvedValue(report);
  jest.mocked(setCaptureBuffer).mockResolvedValue();
  const refreshed = jest.fn().mockResolvedValue(undefined);
  const { result } = renderHook(() =>
    useAudioPerformance("http://host:47990", "test-token", refreshed),
  );
  await act(async () => {
    await result.current.run();
  });
  expect(result.current.report).toEqual(report);
  expect(setCaptureBuffer).not.toHaveBeenCalled();
  await act(async () => {
    await result.current.apply(100);
  });
  expect(setCaptureBuffer).toHaveBeenCalledWith("http://host:47990", 100, "test-token");
  expect(refreshed).toHaveBeenCalledTimes(1);
});

test("a late result from the previous host does not change the new host's settings screen", async () => {
  let finish!: (value: typeof report) => void;
  jest.mocked(runAudioBenchmark).mockReturnValue(
    new Promise((resolve) => {
      finish = resolve;
    }),
  );
  const { result, rerender } = renderHook<ReturnType<typeof useAudioPerformance>, { host: string }>(
    ({ host }) => useAudioPerformance(host, "test-token", async () => {}),
    { initialProps: { host: "http://old:47990" } },
  );
  let pending: Promise<void>;
  act(() => {
    pending = result.current.run();
  });
  rerender({ host: "http://new:47990" });
  await act(async () => {
    finish(report);
    await pending;
  });
  expect(result.current.report).toBeNull();
  expect(setCaptureBuffer).not.toHaveBeenCalled();
});

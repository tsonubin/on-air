import { discoverOnAir } from "@on-air/control-client";
import { act, renderHook } from "@testing-library/react-native";
import { useDiscovery } from "@/hooks/useDiscovery";

jest.mock("@on-air/control-client", () => ({
  ...jest.requireActual("@on-air/control-client"),
  discoverOnAir: jest.fn(),
}));

const STUDIO = { host: "192.168.5.14", port: 47990, name: "studio", version: "0.1.0" };
const DEN = { host: "192.168.5.20", port: 48001, name: "den", version: "0.1.0" };

function deferred<T>() {
  let resolve!: (value: T) => void;
  let reject!: (reason?: unknown) => void;
  const promise = new Promise<T>((res, rej) => {
    resolve = res;
    reject = rej;
  });
  return { promise, resolve, reject };
}

const settle = () => act(async () => {});

beforeEach(() => {
  jest.clearAllMocks();
  jest.mocked(discoverOnAir).mockResolvedValue([STUDIO]);
});

test("scans once when enabled and auto-selects the first hit with its port", async () => {
  const { result, rerender } = renderHook(
    ({ enabled }: { enabled: boolean }) => useDiscovery({ enabled }),
    {
      initialProps: { enabled: false },
    },
  );
  expect(discoverOnAir).not.toHaveBeenCalled();
  rerender({ enabled: true });
  expect(result.current.scanning).toBe(true);
  await settle();
  expect(discoverOnAir).toHaveBeenCalledTimes(1);
  expect(discoverOnAir).toHaveBeenCalledWith({ localIp: "127.0.0.1", extraHosts: [] });
  expect(result.current.scanning).toBe(false);
  expect(result.current.found).toEqual([STUDIO]);
  expect(result.current.host).toBe(STUDIO.host);
  expect(result.current.target).toEqual({ host: STUDIO.host, port: STUDIO.port });
  rerender({ enabled: true });
  expect(discoverOnAir).toHaveBeenCalledTimes(1);
});

test("a late scan still lists what it found but does not replace a typed host", async () => {
  const pending = deferred<(typeof STUDIO)[]>();
  jest.mocked(discoverOnAir).mockReturnValueOnce(pending.promise);
  const { result } = renderHook(() => useDiscovery({ enabled: true }));
  act(() => result.current.changeHost("192.168.5.77:48000"));
  pending.resolve([STUDIO]);
  await settle();
  expect(result.current.found).toEqual([STUDIO]);
  expect(result.current.host).toBe("192.168.5.77:48000");
  expect(result.current.target).toEqual({ host: "192.168.5.77", port: 48000 });
});

test("a chosen desktop survives a rescan that orders hits differently", async () => {
  const { result } = renderHook(() => useDiscovery({ enabled: true }));
  await settle();
  act(() => result.current.selectHost(DEN));
  expect(result.current.target).toEqual({ host: DEN.host, port: DEN.port });
  jest.mocked(discoverOnAir).mockResolvedValueOnce([STUDIO, DEN]);
  await act(() => result.current.scan());
  expect(result.current.found).toEqual([STUDIO, DEN]);
  expect(result.current.target).toEqual({ host: DEN.host, port: DEN.port });
});

test("a typed host is probed alongside the sweep and a failed scan clears the list", async () => {
  const { result } = renderHook(() => useDiscovery({ enabled: true }));
  await settle();
  act(() => result.current.changeHost("10.1.1.5"));
  jest.mocked(discoverOnAir).mockRejectedValueOnce(new Error("no network"));
  await act(() => result.current.scan());
  expect(discoverOnAir).toHaveBeenLastCalledWith({
    localIp: "127.0.0.1",
    extraHosts: ["10.1.1.5"],
  });
  expect(result.current.found).toEqual([]);
  expect(result.current.host).toBe("10.1.1.5");
});

test("reset returns to first-run and allows the automatic scan again", async () => {
  const { result, rerender } = renderHook(
    ({ enabled }: { enabled: boolean }) => useDiscovery({ enabled }),
    {
      initialProps: { enabled: true },
    },
  );
  await settle();
  act(() => result.current.changeHost("10.1.1.5"));
  rerender({ enabled: false });
  act(() => result.current.reset());
  expect(result.current.found).toEqual([]);
  expect(result.current.host).toBe("");
  expect(result.current.target).toBeNull();
  rerender({ enabled: true });
  await settle();
  expect(discoverOnAir).toHaveBeenCalledTimes(2);
  expect(result.current.host).toBe(STUDIO.host);
});

import { act, renderHook } from "@testing-library/react-native";
import * as SecureStore from "expo-secure-store";
import { PERSIST_WARNING, usePairingStore } from "@/hooks/usePairingStore";

const store = SecureStore as typeof SecureStore & { __reset(): void };
const KEY = "on-air.desktop-pairing.v1";
const settle = () => act(async () => {});

beforeEach(() => {
  jest.clearAllMocks();
  jest.spyOn(console, "warn").mockImplementation(() => {});
  store.__reset();
});

afterEach(() => {
  jest.restoreAllMocks();
});

test("hydrates a saved pairing, defaulting the port for records without one", async () => {
  await SecureStore.setItemAsync(KEY, JSON.stringify({ host: "10.0.0.9", token: "tok" }));
  const { result } = renderHook(() => usePairingStore());
  expect(result.current.hydrated).toBe(false);
  await settle();
  expect(result.current.hydrated).toBe(true);
  expect(result.current.pairing).toEqual({ host: "10.0.0.9", port: 47990, token: "tok" });
  expect(result.current.loadFailed).toBe(false);
});

test("an empty keychain hydrates as unpaired, not as an error", async () => {
  const { result } = renderHook(() => usePairingStore());
  await settle();
  expect(result.current.hydrated).toBe(true);
  expect(result.current.pairing).toBeNull();
  expect(result.current.loadFailed).toBe(false);
});

test("a keychain read error is flagged and retried once on the next foreground", async () => {
  await SecureStore.setItemAsync(KEY, JSON.stringify({ host: "10.0.0.9", port: 5, token: "t" }));
  jest.mocked(SecureStore.getItemAsync).mockRejectedValueOnce(new Error("keychain locked"));
  const { result, rerender } = renderHook(
    ({ appActive }: { appActive: boolean }) => usePairingStore({ appActive }),
    {
      initialProps: { appActive: true },
    },
  );
  await settle();
  expect(result.current.hydrated).toBe(true);
  expect(result.current.pairing).toBeNull();
  expect(result.current.loadFailed).toBe(true);
  expect(SecureStore.getItemAsync).toHaveBeenCalledTimes(1);

  rerender({ appActive: false });
  rerender({ appActive: true });
  await settle();
  expect(SecureStore.getItemAsync).toHaveBeenCalledTimes(2);
  expect(result.current.pairing).toEqual({ host: "10.0.0.9", port: 5, token: "t" });
  expect(result.current.loadFailed).toBe(false);

  rerender({ appActive: false });
  rerender({ appActive: true });
  await settle();
  expect(SecureStore.getItemAsync).toHaveBeenCalledTimes(2);
});

test("save keeps the pairing in memory when persistence fails, with a warning", async () => {
  jest.mocked(SecureStore.setItemAsync).mockRejectedValueOnce(new Error("no keychain"));
  const { result } = renderHook(() => usePairingStore());
  await settle();
  act(() => result.current.save({ host: "10.0.0.9", port: 47990, token: "fresh" }));
  expect(result.current.pairing?.token).toBe("fresh");
  await settle();
  expect(result.current.pairing?.token).toBe("fresh");
  expect(result.current.warning).toBe(PERSIST_WARNING);
  expect(console.warn).toHaveBeenCalled();
});

test("save persists host, port and token; clear removes the record", async () => {
  const { result } = renderHook(() => usePairingStore());
  await settle();
  act(() => result.current.save({ host: "10.0.0.9", port: 48000, token: "fresh" }));
  await settle();
  expect(SecureStore.setItemAsync).toHaveBeenCalledWith(
    KEY,
    JSON.stringify({ host: "10.0.0.9", port: 48000, token: "fresh" }),
    expect.objectContaining({ keychainAccessible: SecureStore.WHEN_UNLOCKED_THIS_DEVICE_ONLY }),
  );
  expect(result.current.warning).toBeNull();
  act(() => result.current.clear());
  expect(result.current.pairing).toBeNull();
  await settle();
  expect(SecureStore.deleteItemAsync).toHaveBeenCalledWith(KEY);
});

test("a late keychain read cannot overwrite a pairing saved in the meantime", async () => {
  let release!: (value: string | null) => void;
  jest
    .mocked(SecureStore.getItemAsync)
    .mockImplementationOnce(() => new Promise((resolve) => (release = resolve)));
  const { result } = renderHook(() => usePairingStore());
  act(() => result.current.save({ host: "10.0.0.9", port: 47990, token: "new" }));
  release(JSON.stringify({ host: "10.0.0.1", port: 47990, token: "old" }));
  await settle();
  expect(result.current.pairing?.token).toBe("new");
});

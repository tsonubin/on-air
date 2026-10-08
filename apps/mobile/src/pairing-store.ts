import { DEFAULT_PORT } from "@on-air/api-types";
import * as SecureStore from "expo-secure-store";

const PAIRING_KEY = "on-air.desktop-pairing.v1";

export type SavedPairing = {
  host: string;
  port: number;
  token: string;
};

/**
 * `none` is an empty or unusable record; `error` is a keychain read that
 * threw, which says nothing about whether a pairing exists.
 */
export type LoadedPairing =
  | { status: "saved"; pairing: SavedPairing }
  | { status: "none" }
  | { status: "error"; error: unknown };

function decode(raw: string): SavedPairing | null {
  let saved: Partial<SavedPairing>;
  try {
    saved = JSON.parse(raw) as Partial<SavedPairing>;
  } catch {
    return null;
  }
  if (typeof saved.host !== "string" || typeof saved.token !== "string") return null;
  if (!saved.host.trim() || !saved.token.trim()) return null;
  // Records written before the port was persisted used the default port.
  const port =
    typeof saved.port === "number" && Number.isInteger(saved.port) && saved.port > 0
      ? saved.port
      : DEFAULT_PORT;
  return { host: saved.host, port, token: saved.token };
}

export async function loadPairing(): Promise<LoadedPairing> {
  let raw: string | null;
  try {
    raw = await SecureStore.getItemAsync(PAIRING_KEY);
  } catch (error) {
    return { status: "error", error };
  }
  if (!raw) return { status: "none" };
  const pairing = decode(raw);
  return pairing ? { status: "saved", pairing } : { status: "none" };
}

export async function savePairing(pairing: SavedPairing): Promise<void> {
  await SecureStore.setItemAsync(PAIRING_KEY, JSON.stringify(pairing), {
    keychainAccessible: SecureStore.WHEN_UNLOCKED_THIS_DEVICE_ONLY,
  });
}

export async function clearPairing(): Promise<void> {
  await SecureStore.deleteItemAsync(PAIRING_KEY);
}

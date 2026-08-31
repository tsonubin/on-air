import * as SecureStore from "expo-secure-store";

const PAIRING_KEY = "on-air.desktop-pairing.v1";

export type SavedPairing = {
  host: string;
  token: string;
};

export async function loadPairing(): Promise<SavedPairing | null> {
  try {
    const raw = await SecureStore.getItemAsync(PAIRING_KEY);
    if (!raw) return null;
    const saved = JSON.parse(raw) as Partial<SavedPairing>;
    if (typeof saved.host !== "string" || typeof saved.token !== "string") return null;
    if (!saved.host.trim() || !saved.token.trim()) return null;
    return { host: saved.host, token: saved.token };
  } catch {
    return null;
  }
}

export async function savePairing(pairing: SavedPairing): Promise<void> {
  await SecureStore.setItemAsync(PAIRING_KEY, JSON.stringify(pairing), {
    keychainAccessible: SecureStore.WHEN_UNLOCKED_THIS_DEVICE_ONLY,
  });
}

export async function clearPairing(): Promise<void> {
  await SecureStore.deleteItemAsync(PAIRING_KEY);
}

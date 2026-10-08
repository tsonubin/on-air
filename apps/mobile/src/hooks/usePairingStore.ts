import { useCallback, useEffect, useRef, useState } from "react";
import { clearPairing, loadPairing, type SavedPairing, savePairing } from "@/pairing-store";

export const PERSIST_WARNING =
  "Connected, but this phone could not save the pairing. You may have to pair again after reopening the app.";

export type PairingStore = {
  /** The first keychain read has settled (with a record, nothing, or an error). */
  hydrated: boolean;
  /** In-memory truth: set the moment a PIN is verified, before persistence. */
  pairing: SavedPairing | null;
  /** The last keychain read threw; one retry runs on the next foreground. */
  loadFailed: boolean;
  /** Non-fatal: the pairing works for this launch but could not be persisted. */
  warning: string | null;
  save(pairing: SavedPairing): void;
  clear(): void;
};

/**
 * Owns the saved desktop pairing. Memory is updated synchronously; the
 * keychain is best-effort so a failed write never discards a verified token.
 */
export function usePairingStore({ appActive = true }: { appActive?: boolean } = {}): PairingStore {
  const [hydrated, setHydrated] = useState(false);
  const [pairing, setPairing] = useState<SavedPairing | null>(null);
  const [loadFailed, setLoadFailed] = useState(false);
  const [warning, setWarning] = useState<string | null>(null);
  const generation = useRef(0);
  const retryPending = useRef(false);
  const mounted = useRef(true);

  const load = useCallback(async () => {
    const startedIn = generation.current;
    const loaded = await loadPairing();
    if (!mounted.current || startedIn !== generation.current) return;
    if (loaded.status === "saved") {
      setPairing(loaded.pairing);
      setLoadFailed(false);
    } else if (loaded.status === "none") {
      setLoadFailed(false);
    } else {
      console.warn("on-air: could not read the saved pairing", loaded.error);
      setLoadFailed(true);
      retryPending.current = true;
    }
    setHydrated(true);
  }, []);

  useEffect(() => {
    mounted.current = true;
    void load();
    return () => {
      mounted.current = false;
    };
  }, [load]);

  useEffect(() => {
    if (!appActive || !retryPending.current) return;
    retryPending.current = false;
    void load();
  }, [appActive, load]);

  const save = useCallback((next: SavedPairing) => {
    generation.current += 1;
    retryPending.current = false;
    setPairing(next);
    setLoadFailed(false);
    setWarning(null);
    setHydrated(true);
    void savePairing(next).catch((error: unknown) => {
      console.warn("on-air: could not persist the pairing", error);
      if (mounted.current) setWarning(PERSIST_WARNING);
    });
  }, []);

  const clear = useCallback(() => {
    generation.current += 1;
    retryPending.current = false;
    setPairing(null);
    setWarning(null);
    void clearPairing().catch(() => {
      /* Nothing to fall back to: the next launch loads whatever is still there. */
    });
  }, []);

  return { hydrated, pairing, loadFailed, warning, save, clear };
}

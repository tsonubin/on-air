/**
 * Thin bridge to the Tauri shell. The page also runs in a plain browser
 * (Vite dev server, Playwright), where `@tauri-apps/api` has nothing to talk
 * to, so every call goes through a dynamic import and rejects cleanly outside
 * the shell. Command names match `apps/desktop/src-tauri/src/lib.rs`.
 */
export function inTauri(): boolean {
  return typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;
}

export class NotInTauriError extends Error {
  constructor() {
    super("Not running inside the on-air desktop shell");
    this.name = "NotInTauriError";
  }
}

export async function invokeCommand<T>(
  command: string,
  args?: Record<string, unknown>,
): Promise<T> {
  if (!inTauri()) throw new NotInTauriError();
  const { invoke } = await import("@tauri-apps/api/core");
  return invoke<T>(command, args);
}

/** Subscribes to a shell event; resolves to a no-op outside Tauri. */
export async function listenToShell<T>(
  event: string,
  handler: (payload: T) => void,
): Promise<() => void> {
  if (!inTauri()) return () => {};
  const { listen } = await import("@tauri-apps/api/event");
  return listen<T>(event, (e) => handler(e.payload));
}

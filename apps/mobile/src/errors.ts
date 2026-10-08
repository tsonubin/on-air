import { HttpError } from "@on-air/control-client";

/**
 * Copy for a failed call. Branches on the error envelope's `code` (falling
 * back to the status for cores that predate the envelope); never on message
 * text.
 */
export function friendlyError(error: unknown, action = "complete that action"): string {
  if (error instanceof HttpError) {
    switch (error.code) {
      case "invalid_pin":
        return "That code was not accepted. Check the code on your Mac and try again.";
      case "not_paired":
        return "Pairing expired or the code was not accepted. Pair again.";
      case "pin_lockout":
        return "Too many pairing attempts. Wait a moment and try again.";
      case "service_paused":
        return "The desktop service is paused. Turn it on from the tray menu.";
      case "not_found":
      case "no_active_output":
        return "That device is no longer available. Refresh and try again.";
      case "transport_unreachable":
        return "The speaker did not answer. Check that it is powered on and try again.";
      default:
        break;
    }
    if (error.status === 401) return "Pairing expired or the code was not accepted. Pair again.";
    if (error.status === 429) return "Too many pairing attempts. Wait a moment and try again.";
    if (error.status === 503)
      return "The desktop service is paused. Turn it on from the tray menu.";
    if (error.status === 404) return "That device is no longer available. Refresh and try again.";
  }
  return `Could not ${action}. Check that both devices are on the same Wi-Fi and try again.`;
}

export function isUnauthorized(error: unknown): boolean {
  return error instanceof HttpError && error.status === 401;
}

export function isPaused(error: unknown): boolean {
  return error instanceof HttpError && (error.code === "service_paused" || error.status === 503);
}

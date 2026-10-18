import { errorCopy, HttpError, isServicePaused, isUnauthorized } from "@on-air/control-client";

export { isServicePaused, isUnauthorized };

const WIFI_HINT = "Check that both devices are on the same Wi-Fi and try again.";

/**
 * A rejected request (`invalid_request`) is the one envelope whose message is
 * written for the listener: the core explains what it refused and what to
 * choose instead (why `stereo` cannot play on a mono speaker, say). Show that
 * explanation; every other code keeps the curated copy below.
 */
function coreExplanation(error: unknown): string | null {
  if (!(error instanceof HttpError) || error.code !== "invalid_request") return null;
  try {
    const parsed: unknown = JSON.parse(error.body);
    const message = (parsed as { error?: unknown } | null)?.error;
    return typeof message === "string" && message.trim() !== "" ? message : null;
  } catch {
    return null;
  }
}

/** The shared sentences, reworded where the phone's situation differs. */
export function friendlyError(error: unknown, action = "complete that action"): string {
  const explained = coreExplanation(error);
  if (explained) return explained;
  return errorCopy(error, {
    codes: {
      invalid_pin: "That code was not accepted. Check the code on the desktop and try again.",
      not_paired: "Pairing expired or the code was not accepted. Pair again.",
      pin_lockout: "Too many pairing attempts. Wait a moment and try again.",
      service_paused: "The desktop service is paused. Turn it on from the tray menu.",
      not_found: "That device is no longer available. Refresh and try again.",
      transport_unreachable:
        "The speaker did not answer. Check that it is powered on and try again.",
    },
    unreachable: `Could not reach the desktop. ${WIFI_HINT}`,
    timeout: `The desktop did not answer in time. ${WIFI_HINT}`,
    fallback: () => `Could not ${action}. ${WIFI_HINT}`,
  });
}

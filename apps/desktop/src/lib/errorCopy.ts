import type { ApiErrorCode } from "@on-air/api-types";
import { HttpError } from "@on-air/control-client";

/**
 * The one place that turns a failure into words. Branches on `HttpError.code`
 * (the core's `{error, code}` envelope) and on error names, never on message
 * text, so copy survives wording changes in the core.
 */
const COPY_BY_CODE: Partial<Record<ApiErrorCode, string>> = {
  service_paused: "The audio service is paused. Turn it on to continue.",
  not_paired: "This window is not paired with the audio service. Restart on-air.",
  invalid_pin: "That PIN did not match. Check the code on the speaker and try again.",
  pin_lockout: "Too many PIN attempts. Wait a minute, then try again.",
  transport_unreachable: "The speaker did not answer. Check it is on and on this network.",
  conflict: "Another output is still switching. Try again in a moment.",
  no_active_output: "Nothing is playing yet. Pick a speaker first.",
  not_found: "That device is no longer available. Refresh devices and try again.",
  invalid_request: "The audio service rejected that request.",
  forbidden: "The audio service refused that request.",
  method_not_allowed: "The audio service does not support that action.",
  internal: "The audio service hit an internal error.",
};

const COPY_BY_STATUS: Record<number, string> = {
  401: COPY_BY_CODE.not_paired as string,
  404: COPY_BY_CODE.not_found as string,
  409: COPY_BY_CODE.conflict as string,
  429: COPY_BY_CODE.pin_lockout as string,
  502: COPY_BY_CODE.transport_unreachable as string,
  503: COPY_BY_CODE.service_paused as string,
};

export const UNREACHABLE_COPY = "Cannot reach the audio service on this computer.";
export const TIMEOUT_COPY = "The audio service did not answer in time.";
export const GENERIC_COPY = "Something went wrong.";

function errorName(err: unknown): string | undefined {
  if (err && typeof err === "object" && "name" in err) {
    const name = (err as { name?: unknown }).name;
    return typeof name === "string" ? name : undefined;
  }
  return undefined;
}

/** True when the failure is a `service_paused` (503) response. */
export function isServicePaused(err: unknown): boolean {
  return err instanceof HttpError && (err.code === "service_paused" || err.status === 503);
}

/**
 * True when no response came back at all: the core is down, the port is
 * closed, or the request timed out. `fetch` rejects with a `TypeError` in
 * every browser engine when the connection fails.
 */
export function isUnreachable(err: unknown): boolean {
  if (err instanceof HttpError) return false;
  const name = errorName(err);
  return name === "TypeError" || name === "TimeoutError";
}

export function errorCopy(err: unknown): string {
  if (err instanceof HttpError) {
    const byCode = err.code ? COPY_BY_CODE[err.code] : undefined;
    return byCode ?? COPY_BY_STATUS[err.status] ?? `The audio service answered ${err.status}.`;
  }
  const name = errorName(err);
  if (name === "TimeoutError") return TIMEOUT_COPY;
  if (name === "TypeError") return UNREACHABLE_COPY;
  if (err instanceof Error && err.message) return err.message;
  if (typeof err === "string" && err) return err;
  return GENERIC_COPY;
}

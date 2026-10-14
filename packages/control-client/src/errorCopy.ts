import type { ApiErrorCode } from "@on-air/api-types";
import { HttpError } from "./http.ts";

/**
 * Turns a failed call into words. Branches on the error envelope's `code`
 * (falling back to the HTTP status for cores that predate the envelope) and
 * on error names, never on message text, so copy survives wording changes
 * in the core. Apps override the sentences whose wording is theirs.
 */
export type ErrorCopyByCode = Partial<Record<ApiErrorCode, string>>;

export const ERROR_COPY: Required<
  Pick<
    ErrorCopyByCode,
    | "service_paused"
    | "not_paired"
    | "invalid_pin"
    | "pin_lockout"
    | "transport_unreachable"
    | "conflict"
    | "not_ready"
    | "stream_busy"
    | "no_active_output"
    | "not_found"
    | "invalid_request"
    | "forbidden"
    | "method_not_allowed"
    | "internal"
  >
> = {
  service_paused: "The audio service is paused. Turn it on to continue.",
  not_paired: "Not paired with the audio service. Pair again.",
  invalid_pin: "That PIN did not match. Check the code and try again.",
  pin_lockout: "Too many PIN attempts. Wait a minute, then try again.",
  transport_unreachable: "The speaker did not answer. Check it is on and on this network.",
  conflict: "Another output is still switching. Try again in a moment.",
  not_ready: "That speaker isn't ready. Check it's on and connected, then try again.",
  stream_busy: "The speaker is already playing another stream. Stop it first.",
  no_active_output: "Nothing is playing yet. Pick a speaker first.",
  not_found: "That device is no longer available. Refresh devices and try again.",
  invalid_request: "The audio service rejected that request.",
  forbidden: "The audio service refused that request.",
  method_not_allowed: "The audio service does not support that action.",
  internal: "The audio service hit an internal error.",
};

/** Status fallbacks for cores that answer without a `code`. */
const CODE_BY_STATUS: Record<number, keyof typeof ERROR_COPY> = {
  401: "not_paired",
  404: "not_found",
  409: "conflict",
  429: "pin_lockout",
  502: "transport_unreachable",
  503: "service_paused",
};

export const UNREACHABLE_COPY = "Cannot reach the audio service.";
export const TIMEOUT_COPY = "The audio service did not answer in time.";
export const GENERIC_COPY = "Something went wrong.";

export interface ErrorCopyOptions {
  /** Sentences that replace the shared ones for these codes. */
  codes?: ErrorCopyByCode;
  /** No response at all: connection refused, DNS failure, closed port. */
  unreachable?: string;
  /** The request's timeout elapsed. */
  timeout?: string;
  /** Anything that is neither an HTTP error nor a network failure. */
  fallback?: (err: unknown) => string;
}

export function errorName(err: unknown): string | undefined {
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

/** True when the core rejected the pairing token (401). */
export function isUnauthorized(err: unknown): boolean {
  return err instanceof HttpError && (err.code === "not_paired" || err.status === 401);
}

/**
 * True when no response came back at all: the core is down, the port is
 * closed, or the request timed out. `fetch` rejects with a `TypeError` in
 * every engine when the connection fails.
 */
export function isUnreachable(err: unknown): boolean {
  if (err instanceof HttpError) return false;
  const name = errorName(err);
  return name === "TypeError" || name === "TimeoutError";
}

function defaultFallback(err: unknown): string {
  if (err instanceof Error && err.message) return err.message;
  if (typeof err === "string" && err) return err;
  return GENERIC_COPY;
}

export function errorCopy(err: unknown, options: ErrorCopyOptions = {}): string {
  const codes: ErrorCopyByCode = { ...ERROR_COPY, ...options.codes };
  if (err instanceof HttpError) {
    const byCode = err.code ? codes[err.code] : undefined;
    if (byCode) return byCode;
    const statusCode = CODE_BY_STATUS[err.status];
    return (statusCode && codes[statusCode]) || `The audio service answered ${err.status}.`;
  }
  const name = errorName(err);
  if (name === "TimeoutError") return options.timeout ?? TIMEOUT_COPY;
  if (name === "TypeError") return options.unreachable ?? UNREACHABLE_COPY;
  return (options.fallback ?? defaultFallback)(err);
}

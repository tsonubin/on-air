import assert from "node:assert/strict";
import { test } from "node:test";
import {
  ERROR_COPY,
  errorCopy,
  GENERIC_COPY,
  isServicePaused,
  isUnauthorized,
  isUnreachable,
  TIMEOUT_COPY,
  UNREACHABLE_COPY,
} from "./errorCopy.ts";
import { HttpError } from "./http.ts";

function envelope(status: number, code: string, message = "nope"): HttpError {
  return new HttpError("/api/x", status, JSON.stringify({ error: message, code }));
}

/** Every code the core can send (`ApiErrorCode` in api-types) has a sentence. */
test("every documented error code has copy", () => {
  const documented = [
    "invalid_request",
    "invalid_pin",
    "not_paired",
    "forbidden",
    "not_found",
    "no_active_output",
    "method_not_allowed",
    "conflict",
    "not_ready",
    "stream_busy",
    "pin_lockout",
    "transport_unreachable",
    "service_paused",
    "internal",
  ] as const;
  for (const code of documented) assert.ok(ERROR_COPY[code], `missing copy for ${code}`);
});

test("maps codes to copy and never echoes the server message", () => {
  assert.match(errorCopy(envelope(503, "service_paused", "raw server text")), /paused/i);
  assert.doesNotMatch(errorCopy(envelope(503, "service_paused", "raw server text")), /raw/);
  assert.match(errorCopy(envelope(401, "not_paired")), /pair/i);
  assert.match(errorCopy(envelope(409, "not_ready")), /isn't ready/i);
  assert.match(errorCopy(envelope(409, "conflict")), /switching/i);
});

test("falls back to the status when the code is unknown, then to the status number", () => {
  assert.match(errorCopy(envelope(502, "brand_new_code")), /did not answer/i);
  assert.match(errorCopy(new HttpError("/api/x", 404)), /no longer available/i);
  assert.match(errorCopy(new HttpError("/api/x", 418, "teapot")), /418/);
});

test("apps override sentences per code and the network fallbacks", () => {
  const copy = (err: unknown) =>
    errorCopy(err, {
      codes: { service_paused: "Paused on the desktop." },
      unreachable: "No desktop here.",
      fallback: () => "Could not do it.",
    });
  assert.equal(copy(envelope(503, "service_paused")), "Paused on the desktop.");
  assert.equal(copy(new HttpError("/api/x", 503, "")), "Paused on the desktop.");
  assert.match(copy(envelope(409, "conflict")), /switching/i);
  assert.equal(copy(new TypeError("Failed to fetch")), "No desktop here.");
  assert.equal(copy(new Error("boom")), "Could not do it.");
});

test("recognises a core that is not reachable", () => {
  const network = new TypeError("Failed to fetch");
  assert.equal(errorCopy(network), UNREACHABLE_COPY);
  assert.equal(isUnreachable(network), true);
  const timeout = new DOMException("timed out", "TimeoutError");
  assert.equal(errorCopy(timeout), TIMEOUT_COPY);
  assert.equal(isUnreachable(timeout), true);
  assert.equal(isUnreachable(envelope(503, "service_paused")), false);
});

test("predicates read the code first, then the status", () => {
  assert.equal(isServicePaused(envelope(503, "service_paused")), true);
  assert.equal(isServicePaused(new HttpError("/api/x", 503, "")), true);
  assert.equal(isServicePaused(envelope(409, "conflict")), false);
  assert.equal(isUnauthorized(envelope(401, "not_paired")), true);
  assert.equal(isUnauthorized(new HttpError("/api/x", 401)), true);
  assert.equal(isUnauthorized(envelope(401, "invalid_pin")), true);
  assert.equal(isUnauthorized(envelope(404, "not_found")), false);
});

test("uses the error message or a generic line for unknown values", () => {
  assert.equal(errorCopy(undefined), GENERIC_COPY);
  assert.equal(errorCopy(new Error("plain")), "plain");
});

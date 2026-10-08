import { HttpError } from "@on-air/control-client";
import { describe, expect, it } from "vitest";
import {
  errorCopy,
  GENERIC_COPY,
  isServicePaused,
  isUnreachable,
  TIMEOUT_COPY,
  UNREACHABLE_COPY,
} from "./errorCopy";

function envelope(status: number, code: string, message = "nope"): HttpError {
  return new HttpError("/api/x", status, JSON.stringify({ error: message, code }));
}

describe("errorCopy", () => {
  it("maps service_paused and not_paired codes to user copy", () => {
    expect(errorCopy(envelope(503, "service_paused"))).toMatch(/paused/i);
    expect(errorCopy(envelope(401, "not_paired"))).toMatch(/not paired/i);
  });

  it("never echoes the server message when a code is known", () => {
    const copy = errorCopy(envelope(503, "service_paused", "raw server text"));
    expect(copy).not.toContain("raw server text");
  });

  it("gives not_ready its own copy and keeps conflict for switching", () => {
    const notReady = errorCopy(
      envelope(409, "not_ready", "bluetooth speaker is not connected — pair it first"),
    );
    expect(notReady).toMatch(/isn't ready/i);
    expect(notReady).not.toMatch(/switching/i);
    expect(errorCopy(envelope(409, "conflict"))).toMatch(/switching/i);
  });

  it("falls back to the status when the code is unknown", () => {
    expect(errorCopy(envelope(502, "brand_new_code"))).toMatch(/did not answer/i);
    expect(errorCopy(new HttpError("/api/x", 418, "teapot"))).toContain("418");
  });

  it("recognises a core that is not reachable", () => {
    const network = new TypeError("Failed to fetch");
    expect(errorCopy(network)).toBe(UNREACHABLE_COPY);
    expect(isUnreachable(network)).toBe(true);
    const timeout = new DOMException("timed out", "TimeoutError");
    expect(errorCopy(timeout)).toBe(TIMEOUT_COPY);
    expect(isUnreachable(timeout)).toBe(true);
    expect(isUnreachable(envelope(503, "service_paused"))).toBe(false);
  });

  it("reports pause by code or by status", () => {
    expect(isServicePaused(envelope(503, "service_paused"))).toBe(true);
    expect(isServicePaused(new HttpError("/api/x", 503, ""))).toBe(true);
    expect(isServicePaused(envelope(409, "conflict"))).toBe(false);
  });

  it("uses a generic line for unknown values", () => {
    expect(errorCopy(undefined)).toBe(GENERIC_COPY);
    expect(errorCopy(new Error("plain"))).toBe("plain");
  });
});

import { apiBase, DEFAULT_PORT } from "@on-air/api-types";

/** Where the desktop listens. The port travels with the host everywhere. */
export type DesktopTarget = { host: string; port: number };

/**
 * Parses what a person typed (or a discovery hit) into a target. Accepts
 * `host`, `host:port`, `http://host:port/`, and bracketed IPv6 literals.
 * Returns `null` for an empty string.
 */
export function parseDesktopTarget(
  value: string,
  defaultPort = DEFAULT_PORT,
): DesktopTarget | null {
  const trimmed = value
    .trim()
    .replace(/^https?:\/\//i, "")
    .replace(/\/.*$/, "");
  if (!trimmed) return null;
  const withPort = /^(\[[^\]]+\]|[^:]+):(\d{1,5})$/.exec(trimmed);
  if (withPort) {
    const port = Number(withPort[2]);
    if (port >= 1 && port <= 65535) return { host: withPort[1], port };
  }
  return { host: trimmed, port: defaultPort };
}

export function targetBase(target: DesktopTarget): string {
  return apiBase(target.host, target.port);
}

/** `host` when the port is the default, `host:port` otherwise. */
export function formatDesktopTarget(target: DesktopTarget): string {
  return target.port === DEFAULT_PORT ? target.host : `${target.host}:${target.port}`;
}

export function sameTarget(a: DesktopTarget | null, b: DesktopTarget | null): boolean {
  if (!a || !b) return a === b;
  return a.host === b.host && a.port === b.port;
}

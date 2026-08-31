import { DEFAULT_PORT, type DiscoveredHost, type StatusResponse } from "@on-air/api-types";
import { apiBase, type FetchLike } from "./http.ts";

export type { DiscoveredHost };

const SCAN_TIMEOUT_MS = 350;
const SCAN_CONCURRENCY = 32;

/** True for a private LAN unicast IPv4 we can expand into a /24 probe list. */
export function isLanUnicast(ip: string): boolean {
  const parts = ip.split(".").map(Number);
  if (parts.length !== 4 || parts.some((p) => Number.isNaN(p))) return false;
  const [a, b] = parts;
  if (a === 10) return true;
  if (a === 192 && b === 168) return true;
  if (a === 172 && b >= 16 && b <= 31) return true;
  return false;
}

export function subnetHosts(localIp: string): string[] {
  if (!isLanUnicast(localIp)) return [];
  const parts = localIp.split(".");
  const prefix = `${parts[0]}.${parts[1]}.${parts[2]}`;
  const hosts: string[] = [];
  for (let i = 1; i <= 254; i += 1) {
    hosts.push(`${prefix}.${i}`);
  }
  return hosts;
}

export async function probeOnAir(
  host: string,
  port: number = DEFAULT_PORT,
  timeoutMs: number = SCAN_TIMEOUT_MS,
  fetchImpl: FetchLike = fetch,
): Promise<DiscoveredHost | null> {
  const ctrl = new AbortController();
  const timer = setTimeout(() => ctrl.abort(), timeoutMs);
  try {
    const response = await fetchImpl(`${apiBase(host, port)}/api/status`, {
      signal: ctrl.signal,
    });
    if (!response.ok) return null;
    const body = (await response.json()) as StatusResponse;
    if (body.status !== "ok") return null;
    return { host, port, version: body.version, name: "on-air" };
  } catch {
    return null;
  } finally {
    clearTimeout(timer);
  }
}

async function mapPool<T, R>(items: T[], limit: number, fn: (item: T) => Promise<R | null>): Promise<R[]> {
  const out: R[] = [];
  let index = 0;
  async function worker() {
    while (index < items.length) {
      const current = index;
      index += 1;
      const result = await fn(items[current]);
      if (result) out.push(result);
    }
  }
  await Promise.all(Array.from({ length: Math.min(limit, items.length) }, () => worker()));
  return out;
}

export async function discoverOnAir(opts: {
  localIp?: string;
  port?: number;
  extraHosts?: string[];
  fetchImpl?: FetchLike;
}): Promise<DiscoveredHost[]> {
  const port = opts.port ?? DEFAULT_PORT;
  const fetchImpl = opts.fetchImpl ?? fetch;
  const extra = opts.extraHosts ?? [];
  const hosts = new Set<string>(["127.0.0.1", ...extra]);
  if (opts.localIp) {
    hosts.add(opts.localIp);
    for (const h of subnetHosts(opts.localIp)) hosts.add(h);
  }
  const found = await mapPool([...hosts], SCAN_CONCURRENCY, (host) =>
    probeOnAir(host, port, SCAN_TIMEOUT_MS, fetchImpl),
  );
  const unique = new Map<string, DiscoveredHost>();
  for (const hit of found) {
    unique.set(`${hit.host}:${hit.port}`, hit);
  }
  return [...unique.values()];
}

import { DEFAULT_PORT, type DiscoveredHost, type StatusResponse } from "@on-air/api-types";
import { apiBase, type FetchLike } from "./http.ts";

export type { DiscoveredHost };

// Expo Go development builds and older phones can spend several hundred
// milliseconds crossing the JS/native networking boundary before a LAN
// response is delivered. A sub-second timeout makes a healthy desktop look
// absent and leaves the pairing form pointed at phone-local 127.0.0.1.
const SCAN_TIMEOUT_MS = 1_000;
// Keep the probe fan-out modest for older phones and laptops. Discovery is
// phased and stops launching work after the first responsive batch.
const SCAN_CONCURRENCY = 16;

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

async function mapPool<T, R>(
  items: T[],
  limit: number,
  fn: (item: T) => Promise<R | null>,
  stopAfterFirst = false,
): Promise<R[]> {
  const out: R[] = [];
  let index = 0;
  async function worker() {
    while (index < items.length) {
      if (stopAfterFirst && out.length > 0) return;
      const current = index;
      index += 1;
      const result = await fn(items[current]);
      if (result) out.push(result);
    }
  }
  await Promise.all(Array.from({ length: Math.min(limit, items.length) }, () => worker()));
  return out;
}

function uniqueHits(found: DiscoveredHost[]): DiscoveredHost[] {
  const unique = new Map<string, DiscoveredHost>();
  for (const hit of found) unique.set(`${hit.host}:${hit.port}`, hit);
  return [...unique.values()];
}

function prioritizedSubnet(localIp: string): [string[], string[]] {
  const localHost = Number(localIp.split(".")[3]);
  const hosts = subnetHosts(localIp);
  const quick = hosts.filter((host) => {
    const candidate = Number(host.split(".")[3]);
    return candidate <= 32 || Math.abs(candidate - localHost) <= 8;
  });
  quick.sort((left, right) => {
    const a = Number(left.split(".")[3]);
    const b = Number(right.split(".")[3]);
    if (a === localHost) return -1;
    if (b === localHost) return 1;
    return Math.abs(a - localHost) - Math.abs(b - localHost) || a - b;
  });
  const quickSet = new Set(quick);
  const remaining = hosts
    .filter((host) => !quickSet.has(host))
    .sort((left, right) => {
      const a = Number(left.split(".")[3]);
      const b = Number(right.split(".")[3]);
      return Math.abs(a - localHost) - Math.abs(b - localHost) || a - b;
    });
  return [quick, remaining];
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
  const localIp = opts.localIp;
  const canScanSubnet = Boolean(localIp && isLanUnicast(localIp));
  const directHosts = [...new Set<string>([...(canScanSubnet ? [] : ["127.0.0.1"]), ...extra])];
  const direct = await mapPool(directHosts, Math.min(8, SCAN_CONCURRENCY), (host) =>
    probeOnAir(host, port, SCAN_TIMEOUT_MS, fetchImpl),
  );
  if (direct.length > 0 || !canScanSubnet || !localIp) {
    return uniqueHits(direct);
  }

  const [quick, remaining] = prioritizedSubnet(localIp);
  const quickHits = await mapPool(
    quick,
    SCAN_CONCURRENCY,
    (host) => probeOnAir(host, port, SCAN_TIMEOUT_MS, fetchImpl),
    true,
  );
  if (quickHits.length > 0) return uniqueHits(quickHits);

  const found = await mapPool(
    remaining,
    SCAN_CONCURRENCY,
    (host) => probeOnAir(host, port, SCAN_TIMEOUT_MS, fetchImpl),
    true,
  );
  return uniqueHits(found);
}

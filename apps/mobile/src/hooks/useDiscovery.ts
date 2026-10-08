import { DEFAULT_PORT, type DiscoveredHost } from "@on-air/api-types";
import { discoverOnAir } from "@on-air/control-client";
import * as Network from "expo-network";
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { type DesktopTarget, parseDesktopTarget } from "@/desktop-target";

export type Discovery = {
  /** Hits from the most recent completed scan, whether or not a host was chosen. */
  found: DiscoveredHost[];
  scanning: boolean;
  /** The host field's text: typed by the person or filled from a hit. */
  host: string;
  /** Parsed `host` plus the selected hit's port (or the default). */
  target: DesktopTarget | null;
  scan(): Promise<void>;
  /** The person typed in the host field; later scans stop auto-selecting. */
  changeHost(text: string): void;
  /** The person tapped a discovered desktop. */
  selectHost(hit: DiscoveredHost): void;
  /** Back to the first-run state (after Forget desktop). */
  reset(): void;
};

async function localIpv4(): Promise<string | undefined> {
  try {
    return await Network.getIpAddressAsync();
  } catch {
    return undefined;
  }
}

/**
 * Scans the LAN for a desktop. The newest scan always populates `found`; only
 * the automatic selection of the first hit is gated by whether the person has
 * already typed or chosen a host.
 */
export function useDiscovery({ enabled }: { enabled: boolean }): Discovery {
  const [found, setFound] = useState<DiscoveredHost[]>([]);
  const [scanning, setScanning] = useState(false);
  const [host, setHost] = useState("");
  const [port, setPort] = useState<number>(DEFAULT_PORT);
  const hostRef = useRef(host);
  hostRef.current = host;
  const userEditedHost = useRef(false);
  const scanRevision = useRef(0);
  const scanInFlight = useRef(false);
  const didInitialScan = useRef(false);
  const mounted = useRef(true);

  useEffect(() => {
    mounted.current = true;
    return () => {
      mounted.current = false;
    };
  }, []);

  const scan = useCallback(async () => {
    if (scanInFlight.current) return;
    scanRevision.current += 1;
    const revision = scanRevision.current;
    scanInFlight.current = true;
    setScanning(true);
    try {
      const localIp = await localIpv4();
      const typed = userEditedHost.current ? parseDesktopTarget(hostRef.current) : null;
      const extraHosts = typed && typed.host !== "127.0.0.1" ? [typed.host] : [];
      const hits = await discoverOnAir({ localIp, extraHosts });
      if (!mounted.current || scanRevision.current !== revision) return;
      setFound(hits);
      if (!userEditedHost.current && hits[0]) {
        setHost(hits[0].host);
        setPort(hits[0].port);
      }
    } catch {
      if (mounted.current && scanRevision.current === revision) setFound([]);
    } finally {
      scanInFlight.current = false;
      if (mounted.current) setScanning(false);
    }
  }, []);

  useEffect(() => {
    if (!enabled || didInitialScan.current) return;
    didInitialScan.current = true;
    void scan();
  }, [enabled, scan]);

  const changeHost = useCallback((text: string) => {
    userEditedHost.current = true;
    setHost(text);
    setPort(DEFAULT_PORT);
  }, []);

  const selectHost = useCallback((hit: DiscoveredHost) => {
    userEditedHost.current = true;
    setHost(hit.host);
    setPort(hit.port);
  }, []);

  const reset = useCallback(() => {
    scanRevision.current += 1;
    userEditedHost.current = false;
    didInitialScan.current = false;
    setFound([]);
    setHost("");
    setPort(DEFAULT_PORT);
  }, []);

  const target = useMemo(() => parseDesktopTarget(host, port), [host, port]);

  return { found, scanning, host, target, scan, changeHost, selectHost, reset };
}

import type { WsEvent } from "@on-air/api-types";
import { wsUrl } from "./http.ts";

/** Lifecycle of a subscription as seen by the UI. */
export type ConnectionState = "connecting" | "open" | "reconnecting" | "closed";

export interface EventHandlers {
  onEvent(ev: WsEvent): void;
  /** The socket is open; fires again after every successful reconnect. */
  onOpen?(): void;
  /**
   * The socket closed without `close()` being called: `"remote"` for a server
   * close, `"error"` for a transport error, `"stall"` when no frame arrived
   * within `stallMs`. A reconnect is already scheduled when this fires.
   */
  onClose?(reason: CloseReason): void;
  /**
   * `"connecting"` once at subscribe time, `"open"` on every open,
   * `"reconnecting"` only on the first failure after having been open (not on
   * each retry), `"closed"` once when `close()` is called.
   */
  onStateChange?(state: ConnectionState): void;
}

export type CloseReason = "remote" | "error" | "stall";

/** The subset of the WHATWG WebSocket surface this module touches. */
export interface WebSocketLike {
  onopen: ((ev: unknown) => void) | null;
  onmessage: ((ev: { data: unknown }) => void) | null;
  onclose: ((ev: { code?: number; reason?: string }) => void) | null;
  onerror: ((ev: unknown) => void) | null;
  close(code?: number, reason?: string): void;
  /** Node `ws` exposes server pings as an event; browsers and RN do not. */
  on?(event: "ping", listener: () => void): unknown;
}

export type WebSocketConstructor = new (url: string) => WebSocketLike;

export interface SubscribeOptions {
  /** Injected for tests or runtimes without a global `WebSocket`. */
  WebSocket?: WebSocketConstructor;
  /**
   * Reconnect when no frame arrives for this long. The core sends a ping
   * every 10 s and an event stream while casting; browsers cannot observe
   * pings, so an idle desktop socket only sees events. `0` disables the timer.
   */
  stallMs?: number;
  /** First retry delay; doubles per attempt. */
  minBackoffMs?: number;
  /** Retry delay cap. */
  maxBackoffMs?: number;
  /** Jitter source in [0, 1); injected for deterministic tests. */
  random?: () => number;
}

export interface EventSubscription {
  /** Tears everything down. Never reconnects afterwards; safe to call twice. */
  close(): void;
}

export const DEFAULT_STALL_MS = 20_000;
export const DEFAULT_MIN_BACKOFF_MS = 500;
export const DEFAULT_MAX_BACKOFF_MS = 10_000;
/** Backoff is scaled by a factor in [1 - JITTER, 1 + JITTER]. */
export const BACKOFF_JITTER = 0.2;

export function backoffDelay(
  attempt: number,
  minMs = DEFAULT_MIN_BACKOFF_MS,
  maxMs = DEFAULT_MAX_BACKOFF_MS,
  random: () => number = Math.random,
): number {
  const base = Math.min(minMs * 2 ** attempt, maxMs);
  const factor = 1 - BACKOFF_JITTER + 2 * BACKOFF_JITTER * random();
  return Math.round(base * factor);
}

function parseEvent(data: unknown): WsEvent | undefined {
  if (typeof data !== "string") return undefined;
  try {
    const parsed: unknown = JSON.parse(data);
    if (parsed && typeof parsed === "object" && typeof (parsed as WsEvent).type === "string") {
      return parsed as WsEvent;
    }
  } catch {
    // Not JSON; ignore the frame but let it count as liveness.
  }
  return undefined;
}

/**
 * Subscribe to the core's event stream with automatic reconnect.
 *
 * Owns one socket at a time. On close or error it retries with exponential
 * backoff from 500 ms to a 10 s cap, with +/-20 % jitter, resetting after a
 * successful open. A stall timer closes a socket that has gone quiet for
 * `stallMs` and reconnects. Pure TypeScript: only `WebSocket` and timers.
 */
export function subscribeEvents(
  base: string,
  token: string | undefined,
  handlers: EventHandlers,
  opts: SubscribeOptions = {},
): EventSubscription {
  const WS = opts.WebSocket ?? (globalThis as { WebSocket?: WebSocketConstructor }).WebSocket;
  if (!WS) throw new Error("subscribeEvents: no WebSocket implementation available");
  const url = wsUrl(base, token);
  const stallMs = opts.stallMs ?? DEFAULT_STALL_MS;
  const minBackoffMs = opts.minBackoffMs ?? DEFAULT_MIN_BACKOFF_MS;
  const maxBackoffMs = opts.maxBackoffMs ?? DEFAULT_MAX_BACKOFF_MS;
  const random = opts.random ?? Math.random;

  let stopped = false;
  let attempt = 0;
  let wasOpen = false;
  let socket: WebSocketLike | undefined;
  let retryTimer: ReturnType<typeof setTimeout> | undefined;
  let stallTimer: ReturnType<typeof setTimeout> | undefined;

  const setState = (state: ConnectionState) => handlers.onStateChange?.(state);

  const clearStall = () => {
    if (stallTimer !== undefined) clearTimeout(stallTimer);
    stallTimer = undefined;
  };

  const detach = (ws: WebSocketLike) => {
    ws.onopen = null;
    ws.onmessage = null;
    ws.onclose = null;
    ws.onerror = null;
  };

  const scheduleReconnect = () => {
    if (stopped || retryTimer !== undefined) return;
    const delay = backoffDelay(attempt, minBackoffMs, maxBackoffMs, random);
    attempt += 1;
    retryTimer = setTimeout(() => {
      retryTimer = undefined;
      connect();
    }, delay);
  };

  /** Socket `ws` went away for `reason`; drop it and plan a retry. */
  const lost = (ws: WebSocketLike, reason: CloseReason) => {
    if (stopped || socket !== ws) return;
    socket = undefined;
    clearStall();
    detach(ws);
    try {
      ws.close();
    } catch {
      // Already closed.
    }
    if (wasOpen) {
      wasOpen = false;
      setState("reconnecting");
    }
    handlers.onClose?.(reason);
    scheduleReconnect();
  };

  const connect = () => {
    if (stopped) return;
    let ws: WebSocketLike;
    try {
      ws = new WS(url);
    } catch {
      scheduleReconnect();
      return;
    }
    socket = ws;

    const touch = () => {
      if (stopped || socket !== ws || stallMs <= 0) return;
      clearStall();
      stallTimer = setTimeout(() => lost(ws, "stall"), stallMs);
    };

    ws.onopen = () => {
      if (stopped || socket !== ws) return;
      attempt = 0;
      wasOpen = true;
      touch();
      setState("open");
      handlers.onOpen?.();
    };
    ws.onmessage = (ev) => {
      if (stopped || socket !== ws) return;
      touch();
      const event = parseEvent(ev.data);
      if (event) handlers.onEvent(event);
    };
    ws.onclose = () => lost(ws, "remote");
    ws.onerror = () => lost(ws, "error");
    if (typeof ws.on === "function") ws.on("ping", touch);
  };

  setState("connecting");
  connect();

  return {
    close() {
      if (stopped) return;
      stopped = true;
      if (retryTimer !== undefined) clearTimeout(retryTimer);
      retryTimer = undefined;
      clearStall();
      const ws = socket;
      socket = undefined;
      if (ws) {
        detach(ws);
        try {
          ws.close();
        } catch {
          // Already closed.
        }
      }
      setState("closed");
    },
  };
}

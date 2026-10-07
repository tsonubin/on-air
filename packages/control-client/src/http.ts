import {
  type ActivateInputRequest,
  type ActivateOutputRequest,
  type ActiveInputResponse,
  type ActiveOutput,
  type ActiveOutputResponse,
  type AirPlayMode,
  type AirPlayModeResponse,
  type AirPlayPairRequest,
  type ApiErrorBody,
  apiBase,
  type BluetoothIdRequest,
  type BluetoothListResponse,
  type CdAction,
  type CdControlRequest,
  type CdSimulateRequest,
  type CdStatus,
  DEFAULT_PORT,
  type EqGains,
  type EqResponse,
  type InputsResponse,
  type OutputInfo,
  type OutputsResponse,
  type OutputVolumeResponse,
  type PinResponse,
  type SampleRateResponse,
  type SetEqRequest,
  type SetSampleRateRequest,
  type SetVolumeRequest,
  type StatusResponse,
  type Transport,
  type VerifyRequest,
  type VerifyResponse,
} from "@on-air/api-types";

export { apiBase, DEFAULT_PORT };

export type FetchLike = typeof fetch;

/** Default budget for reads and simple writes. */
export const REQUEST_TIMEOUT_MS = 5_000;
/** Budget for calls that wait on a device handshake (pairing, connecting, going live). */
export const SLOW_REQUEST_TIMEOUT_MS = 60_000;

/** Per-call knobs accepted as the trailing argument of every HTTP function. */
export interface RequestOptions {
  /** Overrides the per-call default (5 s, or 60 s for pairing-style calls). */
  timeoutMs?: number;
  /** Caller cancellation; its `reason` is what the call rejects with. */
  signal?: AbortSignal;
  /** Fetch implementation, for tests and non-global runtimes. */
  fetchImpl?: FetchLike;
}

/**
 * Non-2xx response. `body` is the raw response text; `code` and `message` are
 * filled from the `{error, code}` envelope when the body is one. Branch on
 * `code` (or `status`), never on message text.
 */
export class HttpError extends Error {
  readonly path: string;
  readonly status: number;
  readonly body: string;
  readonly code?: string;

  constructor(path: string, status: number, body = "", code?: string) {
    const envelope = parseErrorBody(body);
    const detail = envelope?.error ? `: ${envelope.error}` : "";
    super(`${path} ${status}${detail}`);
    this.name = "HttpError";
    this.path = path;
    this.status = status;
    this.body = body;
    this.code = code ?? envelope?.code;
  }
}

function parseErrorBody(body: string): ApiErrorBody | undefined {
  if (!body) return undefined;
  try {
    const parsed: unknown = JSON.parse(body);
    if (parsed && typeof parsed === "object") {
      const { error, code } = parsed as Partial<ApiErrorBody>;
      if (typeof code === "string" || typeof error === "string") {
        return {
          error: typeof error === "string" ? error : "",
          code: typeof code === "string" ? code : "",
        };
      }
    }
  } catch {
    // Not the envelope; callers still get the raw text in `body`.
  }
  return undefined;
}

function timeoutError(message: string): Error {
  // DOMException exists in Node 17+, browsers and WebKit; Hermes may lack it.
  if (typeof DOMException === "function") return new DOMException(message, "TimeoutError");
  const error = new Error(message);
  error.name = "TimeoutError";
  return error;
}

function abortError(reason: unknown): unknown {
  if (reason !== undefined) return reason;
  if (typeof DOMException === "function") {
    return new DOMException("The operation was aborted", "AbortError");
  }
  const error = new Error("The operation was aborted");
  error.name = "AbortError";
  return error;
}

/**
 * Combine signals into one. Uses `AbortSignal.any` where the runtime has it
 * (Node 20.3+, modern browsers) and falls back to manual forwarding elsewhere
 * (React Native, older WebKit). `cleanup` detaches the fallback listeners.
 */
function anySignal(signals: AbortSignal[]): { signal: AbortSignal; cleanup(): void } {
  const any = (AbortSignal as { any?: (signals: AbortSignal[]) => AbortSignal }).any;
  if (typeof any === "function") {
    return { signal: any.call(AbortSignal, signals), cleanup: () => {} };
  }
  const controller = new AbortController();
  const forward = (signal: AbortSignal) => () => controller.abort(signal.reason);
  const listeners: Array<[AbortSignal, () => void]> = [];
  for (const signal of signals) {
    if (signal.aborted) {
      controller.abort(signal.reason);
      break;
    }
    const listener = forward(signal);
    signal.addEventListener("abort", listener, { once: true });
    listeners.push([signal, listener]);
  }
  return {
    signal: controller.signal,
    cleanup: () => {
      for (const [signal, listener] of listeners) signal.removeEventListener("abort", listener);
    },
  };
}

function authHeaders(token?: string, json = false): Record<string, string> {
  const headers: Record<string, string> = {};
  if (json) headers["content-type"] = "application/json";
  if (token) headers.authorization = `Bearer ${token}`;
  return headers;
}

function jsonInit(method: string, token: string | undefined, body: unknown): RequestInit {
  return { method, headers: authHeaders(token, true), body: JSON.stringify(body) };
}

async function request<T>(
  base: string,
  path: string,
  init: RequestInit,
  opts: RequestOptions | undefined,
  defaultTimeoutMs = REQUEST_TIMEOUT_MS,
): Promise<T> {
  const url = `${base}${path}`;
  const timeoutMs = opts?.timeoutMs ?? defaultTimeoutMs;
  const fetchImpl = opts?.fetchImpl ?? fetch;
  const callerSignal = opts?.signal;
  if (callerSignal?.aborted) throw abortError(callerSignal.reason);

  const timeoutController = new AbortController();
  let timedOut = false;
  const timer = setTimeout(() => {
    timedOut = true;
    timeoutController.abort(timeoutError(`${url} timed out after ${timeoutMs} ms`));
  }, timeoutMs);
  const combined = anySignal(
    callerSignal ? [callerSignal, timeoutController.signal] : [timeoutController.signal],
  );

  try {
    let response: Response;
    try {
      response = await fetchImpl(url, { ...init, signal: combined.signal });
    } catch (error) {
      // Runtimes disagree on what an aborted fetch rejects with (undici
      // forwards the reason, React Native throws a bare AbortError), so
      // decide from our own bookkeeping rather than from the thrown value.
      if (timedOut) throw timeoutController.signal.reason;
      if (callerSignal?.aborted) throw abortError(callerSignal.reason);
      throw error;
    }
    if (!response.ok) {
      const text = await response.text().catch(() => "");
      throw new HttpError(path, response.status, text);
    }
    if (response.status === 204) {
      return undefined as T;
    }
    const text = await response.text();
    if (!text) return undefined as T;
    return JSON.parse(text) as T;
  } finally {
    clearTimeout(timer);
    combined.cleanup();
  }
}

// ---------------------------------------------------------------------------
// Status and pairing
// ---------------------------------------------------------------------------

export async function fetchStatus(base: string, opts?: RequestOptions): Promise<StatusResponse> {
  return request<StatusResponse>(base, "/api/status", {}, opts);
}

/** Loopback-only on the core: the desktop reads the PIN it shows on screen. */
export async function getPairingPin(base: string, opts?: RequestOptions): Promise<string> {
  const body = await request<PinResponse>(base, "/api/pairing/pin", {}, opts);
  return body.pin;
}

export async function verifyPin(base: string, pin: string, opts?: RequestOptions): Promise<string> {
  const payload: VerifyRequest = { pin };
  const body = await request<VerifyResponse>(
    base,
    "/api/pairing/verify",
    jsonInit("POST", undefined, payload),
    opts,
  );
  return body.token;
}

// ---------------------------------------------------------------------------
// Inputs
// ---------------------------------------------------------------------------

export async function listInputs(
  base: string,
  token?: string,
  opts?: RequestOptions,
): Promise<string[]> {
  const body = await request<InputsResponse>(
    base,
    "/api/inputs",
    { headers: authHeaders(token) },
    opts,
  );
  return body.inputs;
}

export async function getActiveInput(
  base: string,
  token?: string,
  opts?: RequestOptions,
): Promise<ActiveInputResponse> {
  return request<ActiveInputResponse>(
    base,
    "/api/inputs/active",
    { headers: authHeaders(token) },
    opts,
  );
}

export async function activateInput(
  base: string,
  name: string,
  token?: string,
  opts?: RequestOptions,
): Promise<void> {
  const payload: ActivateInputRequest = { name };
  await request(base, "/api/inputs/active", jsonInit("POST", token, payload), opts);
}

// ---------------------------------------------------------------------------
// Outputs
// ---------------------------------------------------------------------------

export async function listOutputs(
  base: string,
  token?: string,
  opts?: RequestOptions,
): Promise<OutputInfo[]> {
  const body = await request<OutputsResponse>(
    base,
    "/api/outputs",
    { headers: authHeaders(token) },
    opts,
  );
  return body.outputs;
}

export async function getActiveOutput(
  base: string,
  token?: string,
  opts?: RequestOptions,
): Promise<ActiveOutput | null> {
  const body = await request<ActiveOutputResponse | null>(
    base,
    "/api/outputs/active",
    { headers: authHeaders(token) },
    opts,
  );
  return body?.active ?? null;
}

/** Goes live on a device. Waits up to 60 s by default: the handshake can be slow. */
export async function activateOutput(
  base: string,
  transport: Transport,
  deviceId: string,
  token?: string,
  opts?: RequestOptions,
): Promise<void> {
  const payload: ActivateOutputRequest = { transport, device_id: deviceId };
  await request(
    base,
    "/api/outputs/active",
    jsonInit("POST", token, payload),
    opts,
    SLOW_REQUEST_TIMEOUT_MS,
  );
}

/** Stops casting. `DELETE /api/outputs/active`. */
export async function deactivateOutput(
  base: string,
  token?: string,
  opts?: RequestOptions,
): Promise<void> {
  await request(
    base,
    "/api/outputs/active",
    { method: "DELETE", headers: authHeaders(token) },
    opts,
  );
}

/** `volume` must be an integer in 0..=100; the core deserialises it as `u8`. */
export async function setVolume(
  base: string,
  volume: number,
  token?: string,
  opts?: RequestOptions,
): Promise<void> {
  const payload: SetVolumeRequest = { volume: Math.round(volume) };
  await request(base, "/api/outputs/active/volume", jsonInit("POST", token, payload), opts);
}

export async function getVolume(
  base: string,
  token?: string,
  opts?: RequestOptions,
): Promise<number> {
  const body = await request<OutputVolumeResponse>(
    base,
    "/api/outputs/active/volume",
    { headers: authHeaders(token) },
    opts,
  );
  return body.volume;
}

// ---------------------------------------------------------------------------
// EQ and sample rate
// ---------------------------------------------------------------------------

export async function getEq(base: string, token?: string, opts?: RequestOptions): Promise<EqGains> {
  const body = await request<EqResponse>(base, "/api/eq", { headers: authHeaders(token) }, opts);
  return body.gains_db;
}

export async function setEq(
  base: string,
  gains: EqGains,
  token?: string,
  opts?: RequestOptions,
): Promise<void> {
  const payload: SetEqRequest = { gains_db: gains };
  await request(base, "/api/eq", jsonInit("PUT", token, payload), opts);
}

export async function getSampleRate(
  base: string,
  token?: string,
  opts?: RequestOptions,
): Promise<SampleRateResponse> {
  return request<SampleRateResponse>(
    base,
    "/api/sample-rate",
    { headers: authHeaders(token) },
    opts,
  );
}

export async function setSampleRate(
  base: string,
  body: SetSampleRateRequest,
  token?: string,
  opts?: RequestOptions,
): Promise<void> {
  await request(base, "/api/sample-rate", jsonInit("PUT", token, body), opts);
}

// ---------------------------------------------------------------------------
// AirPlay and Bluetooth
// ---------------------------------------------------------------------------

export async function getAirplayMode(
  base: string,
  token?: string,
  opts?: RequestOptions,
): Promise<AirPlayMode> {
  const body = await request<AirPlayModeResponse>(
    base,
    "/api/airplay/mode",
    { headers: authHeaders(token) },
    opts,
  );
  return body.mode;
}

/** PIN handshake with an AirPlay receiver. 60 s default timeout. */
export async function pairAirplay(
  base: string,
  deviceId: string,
  pin: string,
  token?: string,
  opts?: RequestOptions,
): Promise<void> {
  const payload: AirPlayPairRequest = { device_id: deviceId, pin };
  await request(
    base,
    "/api/airplay/pair",
    jsonInit("POST", token, payload),
    opts,
    SLOW_REQUEST_TIMEOUT_MS,
  );
}

export async function listBluetooth(
  base: string,
  token?: string,
  opts?: RequestOptions,
): Promise<BluetoothListResponse["devices"]> {
  const body = await request<BluetoothListResponse>(
    base,
    "/api/bluetooth/devices",
    { headers: authHeaders(token) },
    opts,
  );
  return body.devices;
}

/** Bluetooth pairing can involve a user prompt on the desktop. 60 s default timeout. */
export async function pairBluetooth(
  base: string,
  id: string,
  token?: string,
  opts?: RequestOptions,
): Promise<void> {
  const payload: BluetoothIdRequest = { id };
  await request(
    base,
    "/api/bluetooth/pair",
    jsonInit("POST", token, payload),
    opts,
    SLOW_REQUEST_TIMEOUT_MS,
  );
}

/** Connecting waits for the audio endpoint to appear. 60 s default timeout. */
export async function connectBluetooth(
  base: string,
  id: string,
  token?: string,
  opts?: RequestOptions,
): Promise<void> {
  const payload: BluetoothIdRequest = { id };
  await request(
    base,
    "/api/bluetooth/connect",
    jsonInit("POST", token, payload),
    opts,
    SLOW_REQUEST_TIMEOUT_MS,
  );
}

export async function openBluetoothSettings(
  base: string,
  token?: string,
  opts?: RequestOptions,
): Promise<void> {
  await request(
    base,
    "/api/bluetooth/settings",
    { method: "POST", headers: authHeaders(token) },
    opts,
  );
}

// ---------------------------------------------------------------------------
// WebSocket URL
// ---------------------------------------------------------------------------

export function wsUrl(base: string, token?: string): string {
  const ws = base.replace(/^http/i, "ws");
  if (!token) return `${ws}/api/ws`;
  return `${ws}/api/ws?token=${encodeURIComponent(token)}`;
}

// ---------------------------------------------------------------------------
// Audio CD
// ---------------------------------------------------------------------------

export async function getCd(
  base: string,
  token?: string,
  opts?: RequestOptions,
): Promise<CdStatus> {
  return request<CdStatus>(base, "/api/cd", { headers: authHeaders(token) }, opts);
}

export async function controlCd(
  base: string,
  action: CdAction,
  token?: string,
  extra?: Omit<CdControlRequest, "action">,
  opts?: RequestOptions,
): Promise<CdStatus> {
  const payload: CdControlRequest = { action, ...extra };
  return request<CdStatus>(base, "/api/cd/control", jsonInit("POST", token, payload), opts);
}

/** Mock-mode only: insert or eject a simulated disc. `POST /api/mock/cd`. */
export async function simulateCd(
  base: string,
  body: CdSimulateRequest,
  token?: string,
  opts?: RequestOptions,
): Promise<CdStatus> {
  return request<CdStatus>(base, "/api/mock/cd", jsonInit("POST", token, body), opts);
}

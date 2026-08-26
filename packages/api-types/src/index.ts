export interface StatusResponse {
  status: string;
  version: string;
}

// Keep in sync with DEFAULT_PORT in packages/core/src/lib.rs
export const DEFAULT_PORT = 47990;

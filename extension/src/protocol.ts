/** Shared protocol types matching the Rust native messaging side. */

export interface NativeRequest {
  id: string;
  method: string;
  params: Record<string, unknown>;
}

export interface NativeResponse {
  id: string;
  result?: unknown;
  error?: NativeError;
}

export interface NativeError {
  code: string;
  message: string;
}

/** Messages relayed between content script and page context via window.postMessage. */
export interface BridgeMessage {
  type: "jota-request" | "jota-response";
  id: string;
  method?: string;
  params?: Record<string, unknown>;
  result?: unknown;
  error?: NativeError;
}

/** Account info returned by the `connect` method. */
export interface AccountInfo {
  address: string;
  publicKey: string;
  chains: string[];
  features: string[];
}

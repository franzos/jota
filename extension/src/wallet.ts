/**
 * Page-context script: implements the Wallet Standard interface and registers
 * the wallet for dApp discovery. Communicates with the content script via
 * window.postMessage.
 */

import type { BridgeMessage, NativeError, AccountInfo } from "./protocol.js";

// -- Wallet Standard types (minimal subset) --

interface WalletAccount {
  address: string;
  publicKey: Uint8Array;
  chains: readonly string[];
  features: readonly string[];
}

type WalletEventNames = "change";
interface WalletEventsListeners {
  change: (properties: { accounts?: readonly WalletAccount[] }) => void;
}

interface Wallet {
  readonly version: "1.0.0";
  readonly name: string;
  readonly icon: `data:image/${string}`;
  readonly chains: readonly string[];
  readonly features: Record<string, unknown>;
  readonly accounts: readonly WalletAccount[];
}

// -- Helper: send request to content script and wait for response --

const SIGNING_TIMEOUT_MS = 120_000; // 120s for Ledger

let requestCounter = 0;
const pendingRequests = new Map<
  string,
  { resolve: (result: unknown) => void; reject: (error: Error) => void }
>();

function sendRequest(
  method: string,
  params: Record<string, unknown> = {}
): Promise<unknown> {
  const id = `iota-${Date.now()}-${++requestCounter}`;

  return new Promise((resolve, reject) => {
    const timeout = setTimeout(() => {
      pendingRequests.delete(id);
      reject(new Error(`Request ${method} timed out after ${SIGNING_TIMEOUT_MS}ms`));
    }, SIGNING_TIMEOUT_MS);

    pendingRequests.set(id, {
      resolve: (result) => {
        clearTimeout(timeout);
        resolve(result);
      },
      reject: (error) => {
        clearTimeout(timeout);
        reject(error);
      },
    });

    const msg: BridgeMessage = {
      type: "iota-wallet-request",
      id,
      method,
      params,
    };
    window.postMessage(msg, "*");
  });
}

// Listen for responses from the content script
window.addEventListener("message", (event: MessageEvent<BridgeMessage>) => {
  if (event.source !== window) return;
  if (event.data?.type !== "iota-wallet-response") return;

  const { id, result, error } = event.data;
  const pending = pendingRequests.get(id);
  if (!pending) return;

  pendingRequests.delete(id);

  if (error) {
    pending.reject(new Error(`${error.code}: ${error.message}`));
  } else {
    pending.resolve(result);
  }
});

// -- Wallet implementation --

function base64ToBytes(b64: string): Uint8Array {
  const binary = atob(b64);
  const bytes = new Uint8Array(binary.length);
  for (let i = 0; i < binary.length; i++) {
    bytes[i] = binary.charCodeAt(i);
  }
  return bytes;
}

function bytesToBase64(bytes: Uint8Array): string {
  let binary = "";
  for (const b of bytes) {
    binary += String.fromCharCode(b);
  }
  return btoa(binary);
}

// Minimal IOTA icon (blue diamond SVG as data URI)
const WALLET_ICON =
  `data:image/svg+xml,${encodeURIComponent(
    '<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 32 32"><circle cx="16" cy="16" r="15" fill="%232559f5"/><text x="16" y="22" text-anchor="middle" fill="white" font-size="16" font-family="sans-serif" font-weight="bold">I</text></svg>'
  )}` as const;

class IotaDesktopWallet implements Wallet {
  readonly version = "1.0.0" as const;
  readonly name = "IOTA Desktop Wallet";
  readonly icon = WALLET_ICON;
  readonly chains: readonly string[] = ["iota:mainnet", "iota:testnet", "iota:devnet"];

  #accounts: WalletAccount[] = [];
  #listeners: Map<WalletEventNames, Set<WalletEventsListeners[WalletEventNames]>> = new Map();

  get accounts(): readonly WalletAccount[] {
    return this.#accounts;
  }

  get features(): Record<string, unknown> {
    return {
      "standard:connect": { version: "1.0.0", connect: this.#connect.bind(this) },
      "standard:events": {
        version: "1.0.0",
        on: this.#on.bind(this),
      },
      "iota:signTransaction": {
        version: "1.0.0",
        signTransaction: this.#signTransaction.bind(this),
      },
      "iota:signAndExecuteTransaction": {
        version: "1.0.0",
        signAndExecuteTransaction: this.#signAndExecuteTransaction.bind(this),
      },
      "iota:signPersonalMessage": {
        version: "1.0.0",
        signPersonalMessage: this.#signPersonalMessage.bind(this),
      },
    };
  }

  async #connect(): Promise<{ accounts: readonly WalletAccount[] }> {
    const response = (await sendRequest("connect")) as {
      accounts: AccountInfo[];
    };

    this.#accounts = response.accounts.map((a) => ({
      address: a.address,
      publicKey: base64ToBytes(a.publicKey),
      chains: a.chains as readonly string[],
      features: a.features as readonly string[],
    }));

    this.#emit("change", { accounts: this.#accounts });

    return { accounts: this.#accounts };
  }

  async #signTransaction(input: {
    transaction: Uint8Array;
    chain?: string;
  }): Promise<{ bytes: string; signature: string }> {
    const result = (await sendRequest("signTransaction", {
      transaction: bytesToBase64(input.transaction),
      chain: input.chain ?? "iota:mainnet",
    })) as { bytes: string; signature: string };
    return result;
  }

  async #signAndExecuteTransaction(input: {
    transaction: Uint8Array;
    chain?: string;
  }): Promise<{ digest: string; bytes?: string; signature?: string; effects?: string }> {
    const result = (await sendRequest("signAndExecuteTransaction", {
      transaction: bytesToBase64(input.transaction),
      chain: input.chain ?? "iota:mainnet",
    })) as { digest: string; bytes?: string; signature?: string; effects?: string };
    return result;
  }

  async #signPersonalMessage(input: {
    message: Uint8Array;
  }): Promise<{ bytes: string; signature: string }> {
    const result = (await sendRequest("signPersonalMessage", {
      message: bytesToBase64(input.message),
    })) as { bytes: string; signature: string };
    return result;
  }

  #on(
    event: WalletEventNames,
    listener: WalletEventsListeners[WalletEventNames]
  ): () => void {
    let listeners = this.#listeners.get(event);
    if (!listeners) {
      listeners = new Set();
      this.#listeners.set(event, listeners);
    }
    listeners.add(listener);
    return () => {
      listeners!.delete(listener);
    };
  }

  #emit(event: WalletEventNames, ...args: Parameters<WalletEventsListeners[WalletEventNames]>) {
    const listeners = this.#listeners.get(event);
    if (listeners) {
      for (const listener of listeners) {
        try {
          listener(...args);
        } catch (e) {
          console.error("IOTA wallet event listener error:", e);
        }
      }
    }
  }
}

// -- Register wallet via Wallet Standard --
// Uses the official registration protocol from @wallet-standard/wallet.
// 1. Dispatch "wallet-standard:register-wallet" event with a callback in .detail
// 2. Listen for "wallet-standard:app-ready" events from dApp frameworks
// This guarantees registration regardless of load order.

class RegisterWalletEvent extends Event {
  readonly #detail: (api: { register: (...wallets: Wallet[]) => void }) => void;

  get detail() {
    return this.#detail;
  }

  get type() {
    return "wallet-standard:register-wallet" as const;
  }

  constructor(callback: (api: { register: (...wallets: Wallet[]) => void }) => void) {
    super("wallet-standard:register-wallet", {
      bubbles: false,
      cancelable: false,
      composed: false,
    });
    this.#detail = callback;
  }
}

function registerWallet(wallet: Wallet): void {
  const callback = ({ register }: { register: (...wallets: Wallet[]) => void }) =>
    register(wallet);

  try {
    (window as any).dispatchEvent(new RegisterWalletEvent(callback));
  } catch (e) {
    console.error("wallet-standard:register-wallet event could not be dispatched\n", e);
  }

  try {
    (window as any).addEventListener(
      "wallet-standard:app-ready",
      ({ detail: api }: CustomEvent<{ register: (...wallets: Wallet[]) => void }>) =>
        callback(api)
    );
  } catch (e) {
    console.error("wallet-standard:app-ready event listener could not be added\n", e);
  }
}

// Register on load
registerWallet(new IotaDesktopWallet());

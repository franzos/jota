/** Service worker: relays messages between content scripts and the native messaging host. */

import type { NativeRequest, NativeResponse } from "./protocol.js";

const NATIVE_HOST = "org.jota.wallet";

let port: chrome.runtime.Port | null = null;
const pending = new Map<
  string,
  (response: NativeResponse) => void
>();

function ensurePort(): chrome.runtime.Port {
  if (port) return port;

  port = chrome.runtime.connectNative(NATIVE_HOST);

  port.onMessage.addListener((msg: NativeResponse) => {
    const resolver = pending.get(msg.id);
    if (resolver) {
      pending.delete(msg.id);
      resolver(msg);
    }
  });

  port.onDisconnect.addListener(() => {
    const error = chrome.runtime.lastError?.message ?? "Native host disconnected";
    // Reject all pending requests
    for (const [id, resolver] of pending) {
      resolver({
        id,
        error: { code: "INTERNAL_ERROR", message: error },
      });
    }
    pending.clear();
    port = null;
  });

  return port;
}

chrome.runtime.onMessage.addListener(
  (
    message: NativeRequest,
    sender: chrome.runtime.MessageSender,
    sendResponse: (response: NativeResponse) => void
  ) => {
    if (!message.id || !message.method) return false;

    // Inject trusted origin so the GUI knows which site is making the request
    const enriched: NativeRequest = {
      ...message,
      params: { ...message.params, origin: sender.origin ?? sender.url ?? "unknown" },
    };

    const p = ensurePort();
    pending.set(message.id, sendResponse);
    p.postMessage(enriched);

    // Return true to indicate we'll respond asynchronously
    return true;
  }
);

/** Content script: injects wallet.ts into page context and relays messages. */

import type { BridgeMessage, NativeResponse } from "./protocol.js";

// Inject the wallet script into the page context
const script = document.createElement("script");
script.src = chrome.runtime.getURL("dist/wallet.js");
script.onload = () => script.remove();
(document.head || document.documentElement).appendChild(script);

// Listen for requests from the page (wallet.ts)
window.addEventListener("message", (event: MessageEvent<BridgeMessage>) => {
  if (event.source !== window) return;
  if (event.data?.type !== "iota-wallet-request") return;

  const { id, method, params } = event.data;
  if (!id || !method) return;

  // Forward to the background service worker → native host
  chrome.runtime.sendMessage(
    { id, method, params: params ?? {} },
    (response: NativeResponse) => {
      // Relay the response back to the page
      const msg: BridgeMessage = {
        type: "iota-wallet-response",
        id,
        result: response?.result,
        error: response?.error,
      };
      window.postMessage(msg, "*");
    }
  );
});

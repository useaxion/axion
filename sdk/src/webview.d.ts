/**
 * WebView2 IPC surface exposed to the JavaScript context.
 * Available as `window.chrome.webview` when running inside the Axion runtime.
 */
interface ChromeWebView {
  /** Send a string message to the Rust host. */
  postMessage(message: string): void;
  addEventListener(
    type: "message",
    listener: (event: MessageEvent<string>) => void
  ): void;
  removeEventListener(
    type: "message",
    listener: (event: MessageEvent<string>) => void
  ): void;
}

interface Window {
  chrome: {
    webview: ChromeWebView;
  };
}

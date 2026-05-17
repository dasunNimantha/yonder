import React from "react";
import ReactDOM from "react-dom/client";
import { App } from "./App";
import { ApprovalPopup } from "./components/ApprovalPopup/ApprovalPopup";
import "./styles/global.css";

/**
 * We host two windows from the same Vite bundle: the main "Yonder"
 * window and a tiny bottom-right "approval" popup that's used when a
 * file send-request arrives while the main window is hidden in the
 * tray. Both load `index.html` — the window label tells us which UI
 * to mount.
 *
 * Reading the label synchronously via `__TAURI_INTERNALS__` avoids
 * the async getCurrentWindow() flicker on first paint.
 */
function currentWindowLabel(): string {
  try {
    const internals = (window as unknown as {
      __TAURI_INTERNALS__?: { metadata?: { currentWindow?: { label?: string } } };
    }).__TAURI_INTERNALS__;
    return internals?.metadata?.currentWindow?.label ?? "main";
  } catch {
    return "main";
  }
}

const Root = currentWindowLabel() === "approval" ? ApprovalPopup : App;

ReactDOM.createRoot(document.getElementById("root")!).render(
  <React.StrictMode>
    <Root />
  </React.StrictMode>,
);

import { useCallback, useEffect, useState } from "react";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { Minus, Settings as SettingsIcon, Sun, Moon, X } from "lucide-react";

import { inTauri } from "../../lib/tauri";
import { useSettingsStore } from "../../stores/settingsStore";
import "./TitleBar.css";

const appWindow = inTauri ? getCurrentWindow() : null;

interface TitleBarProps {
  onOpenSettings: () => void;
}

export function TitleBar({ onOpenSettings }: TitleBarProps) {
  const [maximized, setMaximized] = useState(false);
  const theme = useSettingsStore((s) => s.settings?.theme ?? "dark");
  const setTheme = useSettingsStore((s) => s.setTheme);

  useEffect(() => {
    if (!appWindow) return;
    appWindow.isMaximized().then(setMaximized);
    const unlistenP = appWindow.onResized(() => {
      appWindow.isMaximized().then(setMaximized);
    });
    return () => {
      unlistenP.then((fn) => fn());
    };
  }, []);

  const handleMinimize = useCallback(() => {
    appWindow?.minimize();
  }, []);

  const handleToggleMaximize = useCallback(() => {
    appWindow?.toggleMaximize();
  }, []);

  const handleClose = useCallback(() => {
    appWindow?.close();
  }, []);

  const toggleTheme = useCallback(() => {
    setTheme(theme === "dark" ? "light" : "dark");
  }, [theme, setTheme]);

  return (
    <div className="titlebar" data-tauri-drag-region>
      <div className="titlebar-left" data-tauri-drag-region>
        <div className="titlebar-mark" aria-hidden="true">
          <svg width="18" height="18" viewBox="0 0 24 24" fill="none">
            <circle cx="12" cy="12" r="3" fill="currentColor" />
            <circle cx="12" cy="12" r="7" stroke="currentColor" strokeWidth="1.4" opacity="0.55" />
            <circle cx="12" cy="12" r="11" stroke="currentColor" strokeWidth="1.4" opacity="0.25" />
          </svg>
        </div>
        <span className="titlebar-title" data-tauri-drag-region>
          Yonder
        </span>
      </div>

      <div className="titlebar-actions">
        <button
          className="titlebar-action"
          onClick={toggleTheme}
          title={theme === "dark" ? "Switch to light theme" : "Switch to dark theme"}
        >
          {theme === "dark" ? <Sun size={14} /> : <Moon size={14} />}
        </button>
        <button
          className="titlebar-action"
          onClick={onOpenSettings}
          title="Settings"
        >
          <SettingsIcon size={14} />
        </button>

        <div className="titlebar-divider" />

        <button
          className="titlebar-btn titlebar-btn-minimize"
          onClick={handleMinimize}
          title="Minimize"
        >
          <Minus size={14} strokeWidth={1.5} />
        </button>
        <button
          className="titlebar-btn titlebar-btn-maximize"
          onClick={handleToggleMaximize}
          title={maximized ? "Restore" : "Maximize"}
        >
          {maximized ? (
            <svg width="11" height="11" viewBox="0 0 11 11" fill="none" stroke="currentColor" strokeWidth="1.2">
              <rect x="0.5" y="2.5" width="8" height="8" rx="0.5" />
              <path d="M2.5 2.5V1a.5.5 0 0 1 .5-.5H10a.5.5 0 0 1 .5.5v7a.5.5 0 0 1-.5.5H8.5" />
            </svg>
          ) : (
            <svg width="11" height="11" viewBox="0 0 11 11" fill="none" stroke="currentColor" strokeWidth="1.2">
              <rect x="0.5" y="0.5" width="10" height="10" rx="0.5" />
            </svg>
          )}
        </button>
        <button
          className="titlebar-btn titlebar-btn-close"
          onClick={handleClose}
          title="Close to tray"
        >
          <X size={15} strokeWidth={1.5} />
        </button>
      </div>
    </div>
  );
}

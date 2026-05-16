import { create } from "zustand";

import { api, type Settings } from "../lib/tauri";

interface SettingsState {
  settings: Settings | null;
  loading: boolean;
  load: () => Promise<void>;
  update: (next: Partial<Settings>) => Promise<void>;
  setTheme: (theme: "dark" | "light") => Promise<void>;
}

function applyTheme(theme: "dark" | "light") {
  try {
    document.documentElement.setAttribute("data-theme", theme);
    localStorage.setItem("yonder-theme", theme);
  } catch {
    /* localStorage might be unavailable in some webview contexts */
  }
}

export const useSettingsStore = create<SettingsState>((set, get) => ({
  settings: null,
  loading: true,

  load: async () => {
    const settings = await api.getSettings();
    applyTheme(settings.theme);
    set({ settings, loading: false });
  },

  update: async (patch) => {
    const current = get().settings;
    if (!current) return;
    const next: Settings = { ...current, ...patch };
    const saved = await api.updateSettings(next);
    applyTheme(saved.theme);
    set({ settings: saved });
  },

  setTheme: async (theme) => {
    applyTheme(theme);
    const current = get().settings;
    if (!current) {
      set({ settings: null });
      return;
    }
    await get().update({ theme });
  },
}));

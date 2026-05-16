import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import * as tauriMod from "../lib/tauri";
import { useSettingsStore } from "./settingsStore";

const baseSettings: tauriMod.Settings = {
  device_id: "dev-1",
  display_name: "Laptop",
  download_dir: "/tmp",
  tcp_port: 53317,
  auto_accept: false,
  start_minimized: false,
  start_on_login: false,
  theme: "dark",
};

describe("settingsStore", () => {
  beforeEach(() => {
    useSettingsStore.setState({ settings: null, loading: true });
    document.documentElement.removeAttribute("data-theme");
    localStorage.clear();
  });

  afterEach(() => {
    vi.restoreAllMocks();
  });

  it("load() pulls settings via api and applies the theme", async () => {
    vi.spyOn(tauriMod.api, "getSettings").mockResolvedValue({
      ...baseSettings,
      theme: "light",
    });

    await useSettingsStore.getState().load();
    const s = useSettingsStore.getState();

    expect(s.settings?.theme).toBe("light");
    expect(s.loading).toBe(false);
    expect(document.documentElement.getAttribute("data-theme")).toBe("light");
    expect(localStorage.getItem("yonder-theme")).toBe("light");
  });

  it("update() merges into current settings, persists via api, and applies theme", async () => {
    useSettingsStore.setState({ settings: { ...baseSettings }, loading: false });

    const spy = vi
      .spyOn(tauriMod.api, "updateSettings")
      .mockImplementation(async (s) => s);

    await useSettingsStore.getState().update({ display_name: "Renamed", theme: "light" });

    expect(spy).toHaveBeenCalledOnce();
    const sentArg = spy.mock.calls[0]![0]!;
    expect(sentArg.display_name).toBe("Renamed");
    expect(sentArg.theme).toBe("light");
    expect(sentArg.device_id).toBe("dev-1");

    expect(useSettingsStore.getState().settings?.display_name).toBe("Renamed");
    expect(document.documentElement.getAttribute("data-theme")).toBe("light");
  });

  it("update() is a no-op when settings haven't been loaded yet", async () => {
    const spy = vi.spyOn(tauriMod.api, "updateSettings");
    await useSettingsStore.getState().update({ display_name: "x" });
    expect(spy).not.toHaveBeenCalled();
  });

  it("setTheme without loaded settings still applies DOM/localStorage", async () => {
    await useSettingsStore.getState().setTheme("light");
    expect(document.documentElement.getAttribute("data-theme")).toBe("light");
    expect(localStorage.getItem("yonder-theme")).toBe("light");
  });

  it("setTheme with loaded settings persists via update()", async () => {
    useSettingsStore.setState({ settings: { ...baseSettings }, loading: false });
    const spy = vi
      .spyOn(tauriMod.api, "updateSettings")
      .mockImplementation(async (s) => s);

    await useSettingsStore.getState().setTheme("light");

    expect(spy).toHaveBeenCalledOnce();
    expect(spy.mock.calls[0]![0]!.theme).toBe("light");
  });
});

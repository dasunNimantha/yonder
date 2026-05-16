import { describe, it, expect } from "vitest";

import { api, inTauri } from "./tauri";

// The setup.ts mocks @tauri-apps/api/core's invoke, but we never reach
// it because window.__TAURI_INTERNALS__ is absent in vitest's jsdom
// environment, so safeInvoke takes the fallback branch on every call.

describe("inTauri detection", () => {
  it("is false when window.__TAURI_INTERNALS__ is absent", () => {
    expect(inTauri).toBe(false);
  });
});

describe("api fallbacks (jsdom mode)", () => {
  it("getSelf returns a synthesized browser identity", async () => {
    const id = await api.getSelf();
    expect(id.id).toBe("browser");
    expect(id.name).toBe("Browser preview");
    expect(id.os).toBe("unknown");
  });

  it("listPeers / listTransfers return empty arrays", async () => {
    expect(await api.listPeers()).toEqual([]);
    expect(await api.listTransfers()).toEqual([]);
  });

  it("getSettings returns sane defaults", async () => {
    const s = await api.getSettings();
    expect(s.tcp_port).toBe(53317);
    expect(s.theme).toBe("dark");
    expect(s.auto_accept).toBe(false);
    expect(s.start_minimized).toBe(false);
    expect(s.start_on_login).toBe(false);
  });

  it("write commands resolve without throwing in jsdom", async () => {
    await expect(api.acceptIncoming("x")).resolves.toBeUndefined();
    await expect(api.rejectIncoming("x")).resolves.toBeUndefined();
    await expect(api.cancelTransfer("x")).resolves.toBeUndefined();
    await expect(api.showMain()).resolves.toBeUndefined();
    await expect(api.hideMain()).resolves.toBeUndefined();
  });
});

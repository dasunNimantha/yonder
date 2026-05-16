import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

export interface Identity {
  id: string;
  name: string;
  os: string;
  version: string;
}

export interface Peer {
  id: string;
  name: string;
  os: string;
  host: string;
  port: number;
  version: string;
}

export interface FileMeta {
  name: string;
  size: number;
  mime?: string | null;
}

export type Direction = "send" | "receive";

export type TransferStatus =
  | "pending"
  | "awaiting-approval"
  | "active"
  | "completed"
  | "cancelled"
  | "failed"
  | "rejected";

export interface Transfer {
  id: string;
  direction: Direction;
  peer_id: string;
  peer_name: string;
  files: FileMeta[];
  total_bytes: number;
  bytes_done: number;
  status: TransferStatus;
  error?: string | null;
  started_at: string;
  finished_at?: string | null;
}

export interface ProgressEvent {
  id: string;
  bytes_done: number;
  total_bytes: number;
  status: TransferStatus;
}

export interface Settings {
  device_id: string;
  display_name: string;
  download_dir: string;
  tcp_port: number;
  auto_accept: boolean;
  start_minimized: boolean;
  start_on_login: boolean;
  theme: "dark" | "light";
}

/**
 * Tiny check so we can degrade gracefully in `npm run dev` outside the
 * Tauri shell (where the IPC bridge isn't available). Every call below
 * no-ops or returns a sane default when the bridge is missing.
 */
export const inTauri =
  typeof window !== "undefined" && !!(window as any).__TAURI_INTERNALS__;

function safeInvoke<T>(cmd: string, args?: Record<string, unknown>, fallback?: T): Promise<T> {
  if (!inTauri) {
    return Promise.resolve(fallback as T);
  }
  return invoke<T>(cmd, args);
}

export const api = {
  getSelf: () =>
    safeInvoke<Identity>("get_self", undefined, {
      id: "browser",
      name: "Browser preview",
      os: "unknown",
      version: "0.0.0",
    }),
  listPeers: () => safeInvoke<Peer[]>("list_peers", undefined, []),
  listTransfers: () => safeInvoke<Transfer[]>("list_transfers", undefined, []),
  sendFiles: (peerId: string, paths: string[]) =>
    safeInvoke<string>("send_files", { peerId, paths }),
  acceptIncoming: (transferId: string) =>
    safeInvoke<void>("accept_incoming", { transferId }),
  rejectIncoming: (transferId: string) =>
    safeInvoke<void>("reject_incoming", { transferId }),
  cancelTransfer: (transferId: string) =>
    safeInvoke<void>("cancel_transfer", { transferId }),
  getSettings: () =>
    safeInvoke<Settings>("get_settings", undefined, {
      device_id: "browser",
      display_name: "Browser preview",
      download_dir: "",
      tcp_port: 53317,
      auto_accept: false,
      start_minimized: false,
      start_on_login: false,
      theme: "dark",
    }),
  updateSettings: (settings: Settings) =>
    safeInvoke<Settings>("update_settings", { newSettings: settings }),
  showMain: () => safeInvoke<void>("show_main"),
  hideMain: () => safeInvoke<void>("hide_main"),
  quitApp: () => safeInvoke<void>("quit_app"),
};

export async function onPeerAdded(cb: (peer: Peer) => void): Promise<UnlistenFn> {
  if (!inTauri) return async () => {};
  return listen<Peer>("peer-added", (e) => cb(e.payload));
}

export async function onPeerRemoved(cb: (id: string) => void): Promise<UnlistenFn> {
  if (!inTauri) return async () => {};
  return listen<{ id: string }>("peer-removed", (e) => cb(e.payload.id));
}

export async function onPeerUpdated(cb: (peer: Peer) => void): Promise<UnlistenFn> {
  if (!inTauri) return async () => {};
  return listen<Peer>("peer-updated", (e) => cb(e.payload));
}

export async function onTransferAdded(cb: (t: Transfer) => void): Promise<UnlistenFn> {
  if (!inTauri) return async () => {};
  return listen<Transfer>("transfer-added", (e) => cb(e.payload));
}

export async function onTransferStarted(cb: (t: Transfer) => void): Promise<UnlistenFn> {
  if (!inTauri) return async () => {};
  return listen<Transfer>("transfer-started", (e) => cb(e.payload));
}

export async function onTransferAwaitingApproval(
  cb: (t: Transfer) => void,
): Promise<UnlistenFn> {
  if (!inTauri) return async () => {};
  return listen<Transfer>("transfer-awaiting-approval", (e) => cb(e.payload));
}

export async function onTransferProgress(
  cb: (e: ProgressEvent) => void,
): Promise<UnlistenFn> {
  if (!inTauri) return async () => {};
  return listen<ProgressEvent>("transfer-progress", (e) => cb(e.payload));
}

export async function onTransferFinished(cb: (t: Transfer) => void): Promise<UnlistenFn> {
  if (!inTauri) return async () => {};
  return listen<Transfer>("transfer-finished", (e) => cb(e.payload));
}

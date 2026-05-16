import { create } from "zustand";

import type { ProgressEvent, Transfer } from "../lib/tauri";

interface TransferState {
  transfers: Transfer[];
  pendingApproval: Transfer | null;
  setTransfers: (transfers: Transfer[]) => void;
  upsertTransfer: (t: Transfer) => void;
  applyProgress: (p: ProgressEvent) => void;
  setPendingApproval: (t: Transfer | null) => void;
  clearCompleted: () => void;
}

const TERMINAL_STATES: Transfer["status"][] = [
  "completed",
  "cancelled",
  "failed",
  "rejected",
];

export const useTransferStore = create<TransferState>((set, get) => ({
  transfers: [],
  pendingApproval: null,

  setTransfers: (transfers) => set({ transfers }),

  upsertTransfer: (t) =>
    set((state) => {
      const idx = state.transfers.findIndex((x) => x.id === t.id);
      if (idx === -1) return { transfers: [...state.transfers, t] };
      const next = state.transfers.slice();
      next[idx] = t;
      return { transfers: next };
    }),

  applyProgress: (p) =>
    set((state) => {
      const idx = state.transfers.findIndex((x) => x.id === p.id);
      if (idx === -1) return state;
      const next = state.transfers.slice();
      const prev = next[idx]!;
      next[idx] = {
        ...prev,
        bytes_done: p.bytes_done,
        total_bytes: p.total_bytes || prev.total_bytes,
        status: p.status,
      };
      return { transfers: next };
    }),

  setPendingApproval: (t) => set({ pendingApproval: t }),

  clearCompleted: () =>
    set({
      transfers: get().transfers.filter((t) => !TERMINAL_STATES.includes(t.status)),
    }),
}));

export function isActive(t: Transfer): boolean {
  return t.status === "active" || t.status === "pending" || t.status === "awaiting-approval";
}

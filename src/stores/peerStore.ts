import { create } from "zustand";

import type { Identity, Peer } from "../lib/tauri";

interface PeerState {
  self: Identity | null;
  peers: Peer[];
  setSelf: (i: Identity | null) => void;
  setPeers: (peers: Peer[]) => void;
  upsertPeer: (peer: Peer) => void;
  removePeer: (id: string) => void;
}

export const usePeerStore = create<PeerState>((set) => ({
  self: null,
  peers: [],
  setSelf: (self) => set({ self }),
  setPeers: (peers) => set({ peers }),
  upsertPeer: (peer) =>
    set((state) => {
      const idx = state.peers.findIndex((p) => p.id === peer.id);
      if (idx === -1) return { peers: [...state.peers, peer] };
      const next = state.peers.slice();
      next[idx] = peer;
      return { peers: next };
    }),
  removePeer: (id) =>
    set((state) => ({ peers: state.peers.filter((p) => p.id !== id) })),
}));

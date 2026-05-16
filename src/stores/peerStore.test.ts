import { beforeEach, describe, expect, it } from "vitest";

import type { Peer } from "../lib/tauri";
import { usePeerStore } from "./peerStore";

function peer(id: string, name = `Device ${id}`): Peer {
  return {
    id,
    name,
    os: "linux",
    version: "0.1.0",
    addresses: ["192.168.1.10:54321"],
  };
}

describe("peerStore", () => {
  beforeEach(() => {
    usePeerStore.setState({ self: null, peers: [] });
  });

  it("starts empty", () => {
    const s = usePeerStore.getState();
    expect(s.peers).toHaveLength(0);
    expect(s.self).toBeNull();
  });

  it("setSelf stores the identity", () => {
    usePeerStore.getState().setSelf({
      id: "me",
      name: "My laptop",
      os: "linux",
      version: "0.1.0",
    });
    expect(usePeerStore.getState().self?.name).toBe("My laptop");
  });

  it("setPeers replaces the peer list", () => {
    usePeerStore.getState().setPeers([peer("a"), peer("b")]);
    expect(usePeerStore.getState().peers).toHaveLength(2);
    usePeerStore.getState().setPeers([peer("c")]);
    expect(usePeerStore.getState().peers).toEqual([peer("c")]);
  });

  it("upsertPeer inserts a new peer", () => {
    usePeerStore.getState().upsertPeer(peer("a"));
    expect(usePeerStore.getState().peers).toEqual([peer("a")]);
  });

  it("upsertPeer replaces an existing peer in place by id", () => {
    usePeerStore.getState().setPeers([peer("a"), peer("b"), peer("c")]);
    const updated = peer("b", "Renamed");
    usePeerStore.getState().upsertPeer(updated);
    const ids = usePeerStore.getState().peers.map((p) => p.id);
    expect(ids).toEqual(["a", "b", "c"]); // order preserved
    expect(usePeerStore.getState().peers[1]!.name).toBe("Renamed");
  });

  it("removePeer drops the matching id and is a no-op for unknown ids", () => {
    usePeerStore.getState().setPeers([peer("a"), peer("b")]);
    usePeerStore.getState().removePeer("a");
    expect(usePeerStore.getState().peers.map((p) => p.id)).toEqual(["b"]);
    usePeerStore.getState().removePeer("nope");
    expect(usePeerStore.getState().peers.map((p) => p.id)).toEqual(["b"]);
  });
});

import { AnimatePresence, motion } from "framer-motion";
import { Radio, RefreshCcw } from "lucide-react";

import type { Identity, Peer } from "../../lib/tauri";
import { PeerCard } from "./PeerCard";
import "./PeerGrid.css";

interface PeerGridProps {
  self: Identity | null;
  peers: Peer[];
  dropTargetPeerId: string | null;
  onPickFilesForPeer: (peer: Peer) => void;
}

export function PeerGrid({ self, peers, dropTargetPeerId, onPickFilesForPeer }: PeerGridProps) {
  return (
    <div className="peer-grid-area">
      <SelfHero self={self} peerCount={peers.length} />

      <div className="peer-grid-wrapper">
        <AnimatePresence>
          {peers.length === 0 ? (
            <motion.div
              key="empty"
              className="peer-grid-empty"
              initial={{ opacity: 0, y: 8 }}
              animate={{ opacity: 1, y: 0 }}
              exit={{ opacity: 0, y: -8 }}
              transition={{ duration: 0.2 }}
            >
              <div className="peer-grid-empty-icon">
                <Radio size={28} />
              </div>
              <div className="peer-grid-empty-title">No devices nearby</div>
              <div className="peer-grid-empty-sub">
                Open Yonder on another device connected to{" "}
                <strong>the same Wi-Fi or LAN</strong> and it will appear here automatically.
              </div>
              <div className="peer-grid-empty-hint">
                <RefreshCcw size={12} /> Discovery is continuous &mdash; nothing to refresh.
              </div>
            </motion.div>
          ) : null}
        </AnimatePresence>

        <motion.div className="peer-grid" layout>
          <AnimatePresence>
            {peers.map((peer) => (
              <motion.div
                key={peer.id}
                layout
                initial={{ opacity: 0, scale: 0.85, y: 10 }}
                animate={{ opacity: 1, scale: 1, y: 0 }}
                exit={{ opacity: 0, scale: 0.85, y: -10 }}
                transition={{ type: "spring", stiffness: 240, damping: 22 }}
              >
                <PeerCard
                  peer={peer}
                  isDropTarget={dropTargetPeerId === peer.id}
                  onClick={() => onPickFilesForPeer(peer)}
                />
              </motion.div>
            ))}
          </AnimatePresence>
        </motion.div>
      </div>
    </div>
  );
}

function SelfHero({ self, peerCount }: { self: Identity | null; peerCount: number }) {
  const status =
    peerCount === 0
      ? "Looking for nearby devices…"
      : peerCount === 1
        ? "1 device nearby"
        : `${peerCount} devices nearby`;

  return (
    <div className="self-hero">
      {/* The pulse runs continuously — we keep advertising over mDNS
          even after peers are found, so the animation matches reality
          and visually reassures the user the app is still "live". */}
      <div className="self-sonar looking">
        <div className="self-sonar-ring r1" />
        <div className="self-sonar-ring r2" />
        <div className="self-sonar-ring r3" />
        <div className="self-avatar" title={self?.name ?? ""}>
          {(self?.name ?? "?").trim().slice(0, 1).toUpperCase() || "?"}
        </div>
      </div>
      <div className="self-name">{self?.name ?? "Yonder Device"}</div>
      <div className="self-status">{status}</div>
    </div>
  );
}

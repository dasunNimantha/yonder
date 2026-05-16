import { Apple, Monitor, Smartphone, HardDrive } from "lucide-react";
import { motion } from "framer-motion";

import type { Peer } from "../../lib/tauri";
import { deterministicHue, monogram } from "../../lib/format";

interface PeerCardProps {
  peer: Peer;
  isDropTarget: boolean;
  onClick: () => void;
}

function osIcon(os: string, size: number) {
  switch (os) {
    case "macos":
      return <Apple size={size} />;
    case "windows":
      return <Monitor size={size} />;
    case "android":
      return <Smartphone size={size} />;
    case "linux":
      return <HardDrive size={size} />;
    default:
      return <HardDrive size={size} />;
  }
}

function osLabel(os: string): string {
  switch (os) {
    case "macos":
      return "macOS";
    case "windows":
      return "Windows";
    case "linux":
      return "Linux";
    case "android":
      return "Android";
    default:
      return "Device";
  }
}

export function PeerCard({ peer, isDropTarget, onClick }: PeerCardProps) {
  const hue = deterministicHue(peer.id);

  return (
    <motion.button
      type="button"
      data-peer-id={peer.id}
      className={`peer-card ${isDropTarget ? "is-drop-target" : ""}`}
      onClick={onClick}
      whileHover={{ y: -3 }}
      transition={{ type: "spring", stiffness: 320, damping: 22 }}
      title={`${peer.name} • ${peer.host}:${peer.port}`}
    >
      <div
        className="peer-avatar"
        style={{
          background: `linear-gradient(135deg,
            hsl(${hue}deg 70% 55%) 0%,
            hsl(${(hue + 40) % 360}deg 70% 45%) 100%)`,
        }}
      >
        {monogram(peer.name, "?")}
        <span className="peer-avatar-os" aria-hidden="true">
          {osIcon(peer.os, 12)}
        </span>
      </div>
      <div className="peer-name" title={peer.name}>
        {peer.name}
      </div>
      <div className="peer-os">{osLabel(peer.os)}</div>

      <div className="peer-card-overlay">
        <span className="peer-card-overlay-text">Drop files to send</span>
      </div>
    </motion.button>
  );
}

import {
  Apple,
  ArrowDownToLine,
  ArrowUpFromLine,
  HardDrive,
  Monitor,
  Send,
  Smartphone,
} from "lucide-react";
import { motion } from "framer-motion";

import type { Peer, Transfer } from "../../lib/tauri";
import { deterministicHue, monogram } from "../../lib/format";

interface PeerCardProps {
  peer: Peer;
  isDropTarget: boolean;
  /** Most recent in-flight transfer with this peer, if any. Drives
   *  the progress ring + glow + direction badge animation. */
  activeTransfer?: Transfer;
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

// Avatar diameter must match `--peer-avatar-size` in PeerGrid.css. The
// SVG ring is drawn slightly larger and sits behind the avatar.
const AVATAR_DIAMETER = 64;
const RING_DIAMETER = 76;
const RING_RADIUS = (RING_DIAMETER - 6) / 2; // 6 = stroke width
const RING_CIRCUMFERENCE = 2 * Math.PI * RING_RADIUS;

export function PeerCard({
  peer,
  isDropTarget,
  activeTransfer,
  onClick,
}: PeerCardProps) {
  const hue = deterministicHue(peer.id);

  const progress =
    activeTransfer && activeTransfer.total_bytes > 0
      ? Math.min(1, activeTransfer.bytes_done / activeTransfer.total_bytes)
      : 0;
  const isTransferring = !!activeTransfer;
  const direction = activeTransfer?.direction;

  return (
    <motion.button
      type="button"
      data-peer-id={peer.id}
      className={`peer-card ${isDropTarget ? "is-drop-target" : ""} ${
        isTransferring ? "is-transferring" : ""
      }`}
      onClick={onClick}
      whileHover={{ y: -3 }}
      transition={{ type: "spring", stiffness: 320, damping: 22 }}
      title={`${peer.name} • ${peer.id.slice(0, 8)}\u2026`}
    >
      <div className="peer-avatar-wrap">
        {isTransferring ? (
          <svg
            className="peer-progress-ring"
            width={RING_DIAMETER}
            height={RING_DIAMETER}
            viewBox={`0 0 ${RING_DIAMETER} ${RING_DIAMETER}`}
            aria-hidden="true"
          >
            <circle
              cx={RING_DIAMETER / 2}
              cy={RING_DIAMETER / 2}
              r={RING_RADIUS}
              className="peer-progress-track"
            />
            <circle
              cx={RING_DIAMETER / 2}
              cy={RING_DIAMETER / 2}
              r={RING_RADIUS}
              className={`peer-progress-fill direction-${direction}`}
              strokeDasharray={RING_CIRCUMFERENCE}
              strokeDashoffset={(1 - progress) * RING_CIRCUMFERENCE}
              transform={`rotate(-90 ${RING_DIAMETER / 2} ${RING_DIAMETER / 2})`}
            />
          </svg>
        ) : null}

        <div
          className="peer-avatar"
          style={{
            width: AVATAR_DIAMETER,
            height: AVATAR_DIAMETER,
            background: `linear-gradient(135deg,
              hsl(${hue}deg 70% 55%) 0%,
              hsl(${(hue + 40) % 360}deg 70% 45%) 100%)`,
          }}
        >
          {monogram(peer.name, "?")}
          <span className="peer-avatar-os" aria-hidden="true">
            {isTransferring ? (
              direction === "send" ? (
                <ArrowUpFromLine size={12} />
              ) : (
                <ArrowDownToLine size={12} />
              )
            ) : (
              osIcon(peer.os, 12)
            )}
          </span>
        </div>
      </div>
      <div className="peer-name" title={peer.name}>
        {peer.name}
      </div>
      <div className="peer-os">
        {isTransferring
          ? `${Math.round(progress * 100)}% ${
              direction === "send" ? "sending" : "receiving"
            }`
          : osLabel(peer.os)}
      </div>

      <div className="peer-card-footer">
        <span className={`peer-live-dot ${isTransferring ? "busy" : ""}`} />
        <span>{isTransferring ? "Transfer in progress" : "Ready to receive"}</span>
      </div>

      <div className="peer-card-cta" aria-hidden="true">
        <Send size={12} />
        <span>Send files</span>
      </div>

      <div className="peer-card-overlay">
        <span className="peer-card-overlay-text">Drop files to send</span>
      </div>
    </motion.button>
  );
}

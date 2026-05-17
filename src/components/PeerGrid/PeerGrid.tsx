import {
  useCallback,
  useLayoutEffect,
  useMemo,
  useRef,
  useState,
} from "react";
import { AnimatePresence, motion } from "framer-motion";
import { Radio, RefreshCcw } from "lucide-react";

import type { Identity, Peer, Transfer } from "../../lib/tauri";
import { PeerCard } from "./PeerCard";
import "./PeerGrid.css";

interface PeerGridProps {
  self: Identity | null;
  peers: Peer[];
  dropTargetPeerId: string | null;
  /** Map of peer.id -> most recent in-flight transfer, used to drive
   *  the per-card transfer animation. */
  activeTransfers: Map<string, Transfer>;
  onPickFilesForPeer: (peer: Peer) => void;
}

export function PeerGrid({
  self,
  peers,
  dropTargetPeerId,
  activeTransfers,
  onPickFilesForPeer,
}: PeerGridProps) {
  const activeTransferCount = activeTransfers.size;
  const areaRef = useRef<HTMLDivElement | null>(null);
  const selfRef = useRef<HTMLDivElement | null>(null);
  const peerRefs = useRef(new Map<string, HTMLDivElement>());
  const [links, setLinks] = useState<TransferLink[]>([]);
  const [overlaySize, setOverlaySize] = useState({ width: 0, height: 0 });

  const activePeerIds = useMemo(
    () => Array.from(activeTransfers.keys()).sort().join("|"),
    [activeTransfers],
  );

  const measureLinks = useCallback(() => {
    const area = areaRef.current;
    const selfEl = selfRef.current;
    if (!area || !selfEl) return;

    const areaRect = area.getBoundingClientRect();
    const selfTarget = selfEl.querySelector<HTMLElement>(".self-avatar") ?? selfEl;
    const selfRect = selfTarget.getBoundingClientRect();
    const selfCenter = centerInArea(selfRect, areaRect, area);
    const selfRadius = Math.min(selfRect.width, selfRect.height) / 2;

    const next: TransferLink[] = [];
    for (const peer of peers) {
      const transfer = activeTransfers.get(peer.id);
      const peerEl = peerRefs.current.get(peer.id);
      if (!transfer || !peerEl) continue;

      const peerTarget = peerEl.querySelector<HTMLElement>(".peer-avatar") ?? peerEl;
      const peerRect = peerTarget.getBoundingClientRect();
      const peerCenter = centerInArea(
        peerRect,
        areaRect,
        area,
      );
      const peerRadius = Math.min(peerRect.width, peerRect.height) / 2;
      const source = transfer.direction === "send" ? selfCenter : peerCenter;
      const target = transfer.direction === "send" ? peerCenter : selfCenter;
      const sourceRadius =
        transfer.direction === "send" ? selfRadius : peerRadius;
      const targetRadius =
        transfer.direction === "send" ? peerRadius : selfRadius;
      const [edgeSource, edgeTarget] = connectCircleEdges(
        source,
        target,
        sourceRadius + 1,
        targetRadius + 1,
      );

      next.push({
        id: transfer.id,
        pathId: `transfer-path-${cssSafeId(transfer.id)}`,
        d: curvedPath(edgeSource, edgeTarget),
        from: edgeSource,
        to: edgeTarget,
        direction: transfer.direction,
        status: transfer.status,
        progress:
          transfer.total_bytes > 0
            ? Math.min(1, transfer.bytes_done / transfer.total_bytes)
            : 0,
      });
    }

    setOverlaySize({ width: area.scrollWidth, height: area.scrollHeight });
    setLinks(next);
  }, [activePeerIds, activeTransfers, peers]);

  useLayoutEffect(() => {
    measureLinks();
    const area = areaRef.current;
    if (!area) return;

    const observer = new ResizeObserver(() => measureLinks());
    observer.observe(area);
    if (selfRef.current) observer.observe(selfRef.current);
    peerRefs.current.forEach((el) => observer.observe(el));

    area.addEventListener("scroll", measureLinks, { passive: true });
    window.addEventListener("resize", measureLinks);

    return () => {
      observer.disconnect();
      area.removeEventListener("scroll", measureLinks);
      window.removeEventListener("resize", measureLinks);
    };
  }, [measureLinks]);

  return (
    <div
      ref={areaRef}
      className={`peer-grid-area ${activeTransferCount > 0 ? "has-transfer" : ""}`}
    >
      <TransferLinksOverlay links={links} size={overlaySize} />

      <SelfHero
        self={self}
        peerCount={peers.length}
        activeTransferCount={activeTransferCount}
        rootRef={(el) => {
          selfRef.current = el;
        }}
      />

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
                ref={(el) => {
                  if (el) peerRefs.current.set(peer.id, el);
                  else peerRefs.current.delete(peer.id);
                }}
                layout
                initial={{ opacity: 0, scale: 0.85, y: 10 }}
                animate={{ opacity: 1, scale: 1, y: 0 }}
                exit={{ opacity: 0, scale: 0.85, y: -10 }}
                transition={{ type: "spring", stiffness: 240, damping: 22 }}
              >
                <PeerCard
                  peer={peer}
                  isDropTarget={dropTargetPeerId === peer.id}
                  activeTransfer={activeTransfers.get(peer.id)}
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

interface Point {
  x: number;
  y: number;
}

interface TransferLink {
  id: string;
  pathId: string;
  d: string;
  from: Point;
  to: Point;
  direction: Transfer["direction"];
  status: Transfer["status"];
  progress: number;
}

function centerInArea(
  rect: DOMRect,
  areaRect: DOMRect,
  area: HTMLDivElement,
): Point {
  return {
    x: rect.left - areaRect.left + area.scrollLeft + rect.width / 2,
    y: rect.top - areaRect.top + area.scrollTop + rect.height / 2,
  };
}

function curvedPath(from: Point, to: Point): string {
  const dx = to.x - from.x;
  const dy = to.y - from.y;
  const length = Math.max(1, Math.hypot(dx, dy));
  const ux = dx / length;
  const uy = dy / length;

  // A connection should feel like it naturally exits one circle and
  // lands on the other. Control points therefore follow the line's
  // tangent first, then add a small perpendicular bend at the middle.
  // This avoids the old "hook" shape caused by vertical-only controls.
  const normal = { x: -uy, y: ux };
  const horizontalBias = Math.sign(dx || 1);
  const bendSign = horizontalBias * (dy < 0 ? -1 : 1);
  const bend = Math.min(84, Math.max(24, length * 0.1)) * bendSign;
  const tangent = Math.min(220, Math.max(74, length * 0.36));

  const c1 = {
    x: from.x + ux * tangent + normal.x * bend,
    y: from.y + uy * tangent + normal.y * bend,
  };
  const c2 = {
    x: to.x - ux * tangent + normal.x * bend,
    y: to.y - uy * tangent + normal.y * bend,
  };

  return `M ${from.x} ${from.y} C ${c1.x} ${c1.y}, ${c2.x} ${c2.y}, ${to.x} ${to.y}`;
}

function connectCircleEdges(
  from: Point,
  to: Point,
  fromRadius: number,
  toRadius: number,
): [Point, Point] {
  const dx = to.x - from.x;
  const dy = to.y - from.y;
  const length = Math.max(1, Math.hypot(dx, dy));
  const ux = dx / length;
  const uy = dy / length;
  return [
    { x: from.x + ux * fromRadius, y: from.y + uy * fromRadius },
    { x: to.x - ux * toRadius, y: to.y - uy * toRadius },
  ];
}

function cssSafeId(id: string): string {
  return id.replace(/[^a-zA-Z0-9_-]/g, "_");
}

function TransferLinksOverlay({
  links,
  size,
}: {
  links: TransferLink[];
  size: { width: number; height: number };
}) {
  if (links.length === 0 || size.width === 0 || size.height === 0) return null;

  return (
    <svg
      className="transfer-network-overlay"
      width={size.width}
      height={size.height}
      viewBox={`0 0 ${size.width} ${size.height}`}
      aria-hidden="true"
    >
      <defs>
        {links.map((link) => (
          <linearGradient
            key={`${link.id}-gradient`}
            id={`${link.pathId}-gradient`}
            gradientUnits="userSpaceOnUse"
          >
            <stop
              offset="0%"
              className={
                link.direction === "send"
                  ? "transfer-gradient-accent"
                  : "transfer-gradient-success"
              }
            />
            <stop
              offset="100%"
              className={
                link.direction === "send"
                  ? "transfer-gradient-success"
                  : "transfer-gradient-accent"
              }
            />
          </linearGradient>
        ))}
      </defs>

      {links.map((link, index) => {
        const waiting =
          link.status === "pending" || link.status === "awaiting-approval";
        return (
        <g
          key={link.id}
          className={`transfer-link-group ${link.direction} ${
            waiting ? "waiting" : "moving"
          }`}
        >
          <path className="transfer-link-glow" d={link.d} />
          <path
            id={link.pathId}
            className="transfer-link-base"
            d={link.d}
            stroke={`url(#${link.pathId}-gradient)`}
          />
          {waiting ? (
            <>
              <path className="transfer-link-wait" d={link.d} pathLength={1} />
              {[0, 1].map((pulse) => (
                <circle key={pulse} className="transfer-link-request" r="5">
                  <animateMotion
                    dur={`${2.4 + index * 0.16}s`}
                    begin={`${pulse * 1.05}s`}
                    repeatCount="indefinite"
                    rotate="auto"
                  >
                    <mpath href={`#${link.pathId}`} />
                  </animateMotion>
                </circle>
              ))}
            </>
          ) : (
            <>
              <path
                className="transfer-link-progress"
                d={link.d}
                stroke={`url(#${link.pathId}-gradient)`}
                pathLength={1}
                strokeDasharray={`${Math.max(0.06, link.progress)} 1`}
              />
              {[0, 1, 2].map((packet) => (
                <circle
                  key={packet}
                  className="transfer-link-packet"
                  r={packet === 1 ? 4.5 : 3.5}
                >
                  <animateMotion
                    dur={`${1.6 + index * 0.12}s`}
                    begin={`${packet * 0.42}s`}
                    repeatCount="indefinite"
                    rotate="auto"
                  >
                    <mpath href={`#${link.pathId}`} />
                  </animateMotion>
                </circle>
              ))}
            </>
          )}
          <circle
            className={`transfer-link-port ${waiting ? "waiting" : "moving"}`}
            cx={link.from.x}
            cy={link.from.y}
            r="4.5"
          />
          <circle
            className={`transfer-link-port ${waiting ? "waiting" : "moving"}`}
            cx={link.to.x}
            cy={link.to.y}
            r="4.5"
          />
        </g>
      )})}
    </svg>
  );
}

function SelfHero({
  self,
  peerCount,
  activeTransferCount,
  rootRef,
}: {
  self: Identity | null;
  peerCount: number;
  activeTransferCount: number;
  rootRef: (el: HTMLDivElement | null) => void;
}) {
  const status =
    activeTransferCount > 0
      ? activeTransferCount === 1
        ? "1 live transfer"
        : `${activeTransferCount} live transfers`
      : peerCount === 0
        ? "Looking for nearby devices…"
        : peerCount === 1
          ? "1 device nearby"
          : `${peerCount} devices nearby`;

  return (
    <div
      ref={rootRef}
      className={`self-hero ${activeTransferCount > 0 ? "is-transferring" : ""}`}
    >
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

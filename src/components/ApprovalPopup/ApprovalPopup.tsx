import { useCallback, useEffect, useMemo, useState } from "react";
import { getCurrentWindow } from "@tauri-apps/api/window";
import {
  Apple,
  ArrowDownToLine,
  Check,
  HardDrive,
  Monitor,
  Smartphone,
  X,
} from "lucide-react";

import {
  api,
  onTransferAwaitingApproval,
  onTransferFinished,
  onTransferStarted,
  type Peer,
  type Transfer,
} from "../../lib/tauri";
import { deterministicHue, formatBytes, monogram } from "../../lib/format";
import "./ApprovalPopup.css";

/**
 * Lightweight always-on-top window pinned bottom-right of the screen,
 * shown whenever an incoming transfer needs user approval. Mirrors
 * what antivirus / system-tray popups do — the user can accept or
 * reject without ever opening the main app window.
 *
 * The window itself is declared in `tauri.conf.json` as the "approval"
 * label, starts hidden, and is `.show()`-ed by the receive handler
 * (`accept.rs::await_approval`) when a request arrives. We hide it
 * again here once the user picks a side, OR when an event indicates
 * the transfer has been resolved elsewhere (e.g. the user accepted
 * via the main window's modal).
 */
export function ApprovalPopup() {
  const [pending, setPending] = useState<Transfer | null>(null);
  const [peerById, setPeerById] = useState<Map<string, Peer>>(new Map());
  const [busy, setBusy] = useState(false);

  // On mount, ask Rust for any outstanding awaiting-approval transfer
  // — covers the case where the window is shown before the event
  // listener has hooked up. We also fetch peers once so we can show
  // the sender's OS icon.
  useEffect(() => {
    let cancelled = false;
    api.listTransfers().then((list) => {
      if (cancelled) return;
      const awaiting = list.find((t) => t.status === "awaiting-approval");
      if (awaiting) setPending(awaiting);
    });
    api.listPeers().then((peers) => {
      if (cancelled) return;
      setPeerById(new Map(peers.map((p) => [p.id, p])));
    });
    return () => {
      cancelled = true;
    };
  }, []);

  useEffect(() => {
    const unlistens: Array<Promise<() => void>> = [];
    unlistens.push(
      onTransferAwaitingApproval((t) => {
        setPending(t);
      }),
      onTransferStarted((t) => {
        // The transfer started — the request was approved (either by
        // us via this popup, by the main window's modal, or by
        // auto-accept). Either way, this popup has nothing more to do.
        if (pending && pending.id === t.id) {
          setPending(null);
          getCurrentWindow().hide();
        }
      }),
      onTransferFinished((t) => {
        // Resolved as rejected/cancelled/failed elsewhere.
        if (pending && pending.id === t.id) {
          setPending(null);
          getCurrentWindow().hide();
        }
      }),
    );
    return () => {
      unlistens.forEach((p) => p.then((fn) => fn()));
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [pending?.id]);

  // Allow Esc to dismiss as Reject — matches OS-native popup UX.
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape" && pending && !busy) {
        void handleReject();
      } else if (e.key === "Enter" && pending && !busy) {
        void handleAccept();
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [pending, busy]);

  const handleAccept = useCallback(async () => {
    if (!pending || busy) return;
    setBusy(true);
    try {
      await api.acceptIncoming(pending.id);
    } catch (e) {
      console.error("accept failed:", e);
    } finally {
      setBusy(false);
      setPending(null);
      try {
        await getCurrentWindow().hide();
      } catch {
        /* might fail outside Tauri */
      }
    }
  }, [pending, busy]);

  const handleReject = useCallback(async () => {
    if (!pending || busy) return;
    setBusy(true);
    try {
      await api.rejectIncoming(pending.id);
    } catch (e) {
      console.error("reject failed:", e);
    } finally {
      setBusy(false);
      setPending(null);
      try {
        await getCurrentWindow().hide();
      } catch {
        /* might fail outside Tauri */
      }
    }
  }, [pending, busy]);

  const peer = pending ? peerById.get(pending.peer_id) : undefined;
  const hue = useMemo(
    () => (pending ? deterministicHue(pending.peer_id) : 220),
    [pending],
  );

  if (!pending) {
    return <div className="approval-popup empty" />;
  }

  const fileSummary =
    pending.files.length === 1
      ? pending.files[0]!.name
      : `${pending.files.length} files`;

  return (
    <div className="approval-popup" data-tauri-drag-region>
      <span className="approval-accent" aria-hidden="true" />

      <div
        className="approval-avatar"
        style={{
          background: `linear-gradient(135deg,
            hsl(${hue}deg 70% 55%) 0%,
            hsl(${(hue + 40) % 360}deg 70% 45%) 100%)`,
        }}
        aria-hidden="true"
      >
        {monogram(pending.peer_name, "?")}
        <span className="approval-avatar-badge">
          {peer ? osIcon(peer.os) : <ArrowDownToLine size={11} />}
        </span>
      </div>

      <div className="approval-body">
        <div className="approval-title" title={pending.peer_name}>
          <strong>{pending.peer_name}</strong>
          <span className="approval-subtitle"> wants to send</span>
        </div>
        <div className="approval-detail" title={fileSummary}>
          <span className="approval-filename">{fileSummary}</span>
          <span className="approval-dot" aria-hidden="true">
            ·
          </span>
          <span className="approval-size">{formatBytes(pending.total_bytes)}</span>
        </div>
      </div>

      <div className="approval-actions">
        <button
          className="approval-btn approval-btn-reject"
          onClick={handleReject}
          disabled={busy}
          title="Reject (Esc)"
          aria-label="Reject"
        >
          <X size={14} strokeWidth={2.2} />
        </button>
        <button
          className="approval-btn approval-btn-accept"
          onClick={handleAccept}
          disabled={busy}
          title="Accept (Enter)"
          autoFocus
        >
          <Check size={14} strokeWidth={2.5} />
          <span>Accept</span>
        </button>
      </div>
    </div>
  );
}

function osIcon(os: string) {
  switch (os) {
    case "macos":
      return <Apple size={11} />;
    case "windows":
      return <Monitor size={11} />;
    case "android":
      return <Smartphone size={11} />;
    case "linux":
      return <HardDrive size={11} />;
    default:
      return <HardDrive size={11} />;
  }
}

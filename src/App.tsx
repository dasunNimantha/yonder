import { useCallback, useEffect, useRef, useState } from "react";
import { open as openDialog } from "@tauri-apps/plugin-dialog";
import { getCurrentWebview } from "@tauri-apps/api/webview";
import { ArrowUpDown, BadgeCheck, Wifi } from "lucide-react";

import { TitleBar } from "./components/TitleBar/TitleBar";
import { PeerGrid } from "./components/PeerGrid/PeerGrid";
import { TransfersPanel } from "./components/TransfersPanel/TransfersPanel";
import { SettingsDialog } from "./components/SettingsDialog/SettingsDialog";
import { ReceivePrompt } from "./components/ReceivePrompt";
import { ToastContainer } from "./components/Toast/Toast";

import {
  api,
  inTauri,
  onPeerAdded,
  onPeerRemoved,
  onPeerUpdated,
  onTransferAdded,
  onTransferAwaitingApproval,
  onTransferFinished,
  onTransferProgress,
  onTransferStarted,
  type Peer,
} from "./lib/tauri";
import { usePeerStore } from "./stores/peerStore";
import { useTransferStore, isActive } from "./stores/transferStore";
import { useSettingsStore } from "./stores/settingsStore";
import { useToastStore } from "./stores/toastStore";
import "./App.css";

export function App() {
  const setSelf = usePeerStore((s) => s.setSelf);
  const self = usePeerStore((s) => s.self);
  const peers = usePeerStore((s) => s.peers);
  const setPeers = usePeerStore((s) => s.setPeers);
  const upsertPeer = usePeerStore((s) => s.upsertPeer);
  const removePeer = usePeerStore((s) => s.removePeer);

  const transfers = useTransferStore((s) => s.transfers);
  const setTransfers = useTransferStore((s) => s.setTransfers);
  const upsertTransfer = useTransferStore((s) => s.upsertTransfer);
  const applyProgress = useTransferStore((s) => s.applyProgress);
  const pendingApproval = useTransferStore((s) => s.pendingApproval);
  const setPendingApproval = useTransferStore((s) => s.setPendingApproval);

  const loadSettings = useSettingsStore((s) => s.load);
  const addToast = useToastStore((s) => s.addToast);

  const [transfersOpen, setTransfersOpen] = useState(false);
  const [settingsOpen, setSettingsOpen] = useState(false);
  const [dropTargetPeerId, setDropTargetPeerId] = useState<string | null>(null);

  // Tracks the peer currently under the drag cursor. We keep it in a
  // ref so the Tauri drag-drop subscription can read the latest value
  // without re-subscribing on every state change.
  const dropTargetRef = useRef<string | null>(null);
  const lastSeenAwaiting = useRef<Set<string>>(new Set());

  // ── Initial load ────────────────────────────────────────────────
  useEffect(() => {
    let cancelled = false;

    (async () => {
      const [s, identity, peerList, transferList] = await Promise.all([
        useSettingsStore.getState().load().then(() => useSettingsStore.getState().settings),
        api.getSelf(),
        api.listPeers(),
        api.listTransfers(),
      ]);
      if (cancelled) return;
      setSelf(identity);
      setPeers(peerList);
      setTransfers(transferList);
      void s;
    })();

    void loadSettings();
    return () => {
      cancelled = true;
    };
  }, [loadSettings, setPeers, setSelf, setTransfers]);

  // ── Event wiring ───────────────────────────────────────────────
  useEffect(() => {
    const unlistens: Array<Promise<() => void>> = [];

    unlistens.push(
      onPeerAdded((peer) => {
        upsertPeer(peer);
      }),
      onPeerUpdated((peer) => {
        upsertPeer(peer);
      }),
      onPeerRemoved((id) => {
        removePeer(id);
      }),
      onTransferAdded((t) => {
        upsertTransfer(t);
        if (t.direction === "send") {
          setTransfersOpen(true);
        }
      }),
      onTransferStarted((t) => {
        upsertTransfer(t);
        // Once a send/receive is approved, clear the pending modal.
        setPendingApproval(null);
        lastSeenAwaiting.current.delete(t.id);
      }),
      onTransferAwaitingApproval((t) => {
        upsertTransfer(t);
        if (lastSeenAwaiting.current.has(t.id)) return;
        lastSeenAwaiting.current.add(t.id);
        setPendingApproval(t);
      }),
      onTransferProgress((p) => {
        applyProgress(p);
      }),
      onTransferFinished((t) => {
        upsertTransfer(t);
        const current = useTransferStore.getState().pendingApproval;
        if (current?.id === t.id) {
          setPendingApproval(null);
        }
        if (t.status === "completed") {
          addToast(
            t.direction === "send"
              ? `Sent ${t.files.length} file${t.files.length === 1 ? "" : "s"} to ${t.peer_name}`
              : `Received ${t.files.length} file${t.files.length === 1 ? "" : "s"} from ${t.peer_name}`,
            "success",
          );
        } else if (t.status === "failed") {
          addToast(t.error ?? "Transfer failed", "error");
        } else if (t.status === "rejected") {
          addToast(
            t.direction === "send"
              ? `${t.peer_name} rejected your files`
              : `Rejected files from ${t.peer_name}`,
            "warning",
          );
        }
      }),
    );

    return () => {
      unlistens.forEach((p) => p.then((fn) => fn()));
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  // ── Actions ────────────────────────────────────────────────────

  const handleSendToPeer = useCallback(
    async (peer: Peer, paths: string[]) => {
      if (paths.length === 0) return;
      try {
        await api.sendFiles(peer.id, paths);
        addToast(
          `Sending ${paths.length} file${paths.length === 1 ? "" : "s"} to ${peer.name}…`,
          "info",
        );
        setTransfersOpen(true);
      } catch (e) {
        addToast(`Could not send: ${e}`, "error");
      }
    },
    [addToast],
  );

  const handlePickFilesForPeer = useCallback(
    async (peer: Peer) => {
      if (!inTauri) {
        addToast(
          "File picker only works inside the Yonder desktop app.",
          "warning",
        );
        return;
      }
      try {
        const picked = await openDialog({
          multiple: true,
          directory: false,
          title: `Send files to ${peer.name}`,
        });
        if (!picked) return;
        const paths = Array.isArray(picked) ? picked : [picked];
        const strings = paths.filter((p): p is string => typeof p === "string");
        if (strings.length === 0) return;
        await handleSendToPeer(peer, strings);
      } catch (e) {
        addToast(`Picker failed: ${e}`, "error");
      }
    },
    [addToast, handleSendToPeer],
  );

  const handleAccept = useCallback(
    async (id: string) => {
      try {
        await api.acceptIncoming(id);
        setPendingApproval(null);
      } catch (e) {
        addToast(`Accept failed: ${e}`, "error");
      }
    },
    [addToast, setPendingApproval],
  );

  const handleReject = useCallback(
    async (id: string) => {
      try {
        await api.rejectIncoming(id);
        setPendingApproval(null);
      } catch (e) {
        addToast(`Reject failed: ${e}`, "error");
      }
    },
    [addToast, setPendingApproval],
  );

  // Block default browser drop behaviour outside drop targets so
  // accidentally missing a peer card doesn't navigate the WebView away.
  useEffect(() => {
    const stop = (e: DragEvent) => {
      e.preventDefault();
    };
    window.addEventListener("dragover", stop);
    window.addEventListener("drop", stop);
    return () => {
      window.removeEventListener("dragover", stop);
      window.removeEventListener("drop", stop);
    };
  }, []);

  // ── Tauri drag-drop: figure out which PeerCard the cursor was over
  //    using elementFromPoint at the drop position, then send files.
  useEffect(() => {
    if (!inTauri) return;
    let unlisten: (() => void) | null = null;
    let cancelled = false;

    const setTarget = (peerId: string | null) => {
      if (dropTargetRef.current !== peerId) {
        dropTargetRef.current = peerId;
        setDropTargetPeerId(peerId);
      }
    };

    const peerAtPoint = (x: number, y: number): string | null => {
      const el = document.elementFromPoint(x, y);
      if (!el) return null;
      const card = (el as HTMLElement).closest<HTMLElement>("[data-peer-id]");
      return card?.getAttribute("data-peer-id") ?? null;
    };

    (async () => {
      const webview = getCurrentWebview();
      const stop = await webview.onDragDropEvent((event) => {
        const p: any = event.payload;
        // Tauri 2 emits "enter" / "over" / "drop" / "leave" with
        // logical position {x, y} relative to the webview.
        if (p.type === "enter" || p.type === "over") {
          const pos = p.position;
          if (pos && typeof pos.x === "number" && typeof pos.y === "number") {
            setTarget(peerAtPoint(pos.x, pos.y));
          }
        } else if (p.type === "leave") {
          setTarget(null);
        } else if (p.type === "drop") {
          const pos = p.position;
          const peerId =
            pos && typeof pos.x === "number" && typeof pos.y === "number"
              ? peerAtPoint(pos.x, pos.y) ?? dropTargetRef.current
              : dropTargetRef.current;
          setTarget(null);
          const paths: string[] = Array.isArray(p.paths) ? p.paths : [];
          if (!peerId) {
            if (paths.length > 0) {
              addToast(
                "Drop the files onto a device card to send them.",
                "warning",
              );
            }
            return;
          }
          const peer = usePeerStore.getState().peers.find((x) => x.id === peerId);
          if (peer && paths.length > 0) {
            void handleSendToPeer(peer, paths);
          }
        }
      });
      if (cancelled) {
        stop();
      } else {
        unlisten = stop;
      }
    })();

    return () => {
      cancelled = true;
      if (unlisten) unlisten();
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  // ── Keyboard shortcuts ─────────────────────────────────────────
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        if (settingsOpen) {
          setSettingsOpen(false);
        } else if (transfersOpen) {
          setTransfersOpen(false);
        }
      } else if ((e.metaKey || e.ctrlKey) && e.key === ",") {
        e.preventDefault();
        setSettingsOpen(true);
      } else if ((e.metaKey || e.ctrlKey) && (e.key === "t" || e.key === "T")) {
        e.preventDefault();
        setTransfersOpen((o) => !o);
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [settingsOpen, transfersOpen]);

  const activeCount = transfers.filter(isActive).length;

  return (
    <>
      <TitleBar onOpenSettings={() => setSettingsOpen(true)} />

      <div className="app-toolbar">
        <div className="app-toolbar-left">
          <Wifi size={14} />
          <span title={self?.id ?? ""}>
            You: <strong>{self?.name ?? "Yonder"}</strong>
          </span>
          <span className="app-toolbar-dot" />
          <BadgeCheck size={14} />
          <span title={self?.id ?? ""}>
            <code className="app-toolbar-id">
              {self?.id ? `${self.id.slice(0, 8)}\u2026` : "\u2026"}
            </code>
          </span>
        </div>
        <button
          className={`app-toolbar-btn ${transfersOpen ? "active" : ""}`}
          onClick={() => setTransfersOpen((o) => !o)}
          title="Open transfers"
        >
          <ArrowUpDown size={14} />
          <span>Transfers</span>
          {activeCount > 0 ? <span className="app-toolbar-badge">{activeCount}</span> : null}
        </button>
      </div>

      <main className={`app-main ${transfersOpen ? "with-side-panel" : ""}`}>
        <PeerGrid
          self={self}
          peers={peers}
          dropTargetPeerId={dropTargetPeerId}
          onPickFilesForPeer={handlePickFilesForPeer}
        />
      </main>

      <TransfersPanel open={transfersOpen} onClose={() => setTransfersOpen(false)} />

      <SettingsDialog open={settingsOpen} onClose={() => setSettingsOpen(false)} />

      <ReceivePrompt
        transfer={pendingApproval}
        onAccept={handleAccept}
        onReject={handleReject}
      />

      <ToastContainer />
    </>
  );
}

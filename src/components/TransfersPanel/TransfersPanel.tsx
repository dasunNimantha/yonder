import { useEffect, useMemo, useRef, useState } from "react";
import { AnimatePresence, motion } from "framer-motion";
import {
  ArrowDownToLine,
  ArrowUpFromLine,
  CheckCircle2,
  Eraser,
  Loader2,
  X,
  XCircle,
} from "lucide-react";

import type { Transfer } from "../../lib/tauri";
import { formatBytes, formatPercent, formatSpeed } from "../../lib/format";
import { useTransferStore, isActive } from "../../stores/transferStore";
import { api } from "../../lib/tauri";
import "./TransfersPanel.css";

interface TransfersPanelProps {
  open: boolean;
  onClose: () => void;
}

export function TransfersPanel({ open, onClose }: TransfersPanelProps) {
  const transfers = useTransferStore((s) => s.transfers);
  const clearCompleted = useTransferStore((s) => s.clearCompleted);

  const { active, history } = useMemo(() => {
    const a: Transfer[] = [];
    const h: Transfer[] = [];
    for (const t of transfers) {
      if (isActive(t)) a.push(t);
      else h.push(t);
    }
    h.sort((x, y) => +new Date(y.finished_at ?? y.started_at) - +new Date(x.finished_at ?? x.started_at));
    a.sort((x, y) => +new Date(y.started_at) - +new Date(x.started_at));
    return { active: a, history: h };
  }, [transfers]);

  return (
    <AnimatePresence>
      {open ? (
        <motion.aside
          className="transfers-panel"
          initial={{ x: 360, opacity: 0 }}
          animate={{ x: 0, opacity: 1 }}
          exit={{ x: 360, opacity: 0 }}
          transition={{ type: "spring", stiffness: 240, damping: 28 }}
        >
          <header className="transfers-panel-header">
            <div className="transfers-panel-title">Transfers</div>
            <div className="transfers-panel-actions">
              <button
                className="transfers-panel-icon-btn"
                onClick={clearCompleted}
                title="Clear finished transfers"
                disabled={history.length === 0}
              >
                <Eraser size={14} />
              </button>
              <button className="transfers-panel-icon-btn" onClick={onClose} title="Close">
                <X size={16} />
              </button>
            </div>
          </header>

          <div className="transfers-panel-body">
            <Section title="Active" items={active} empty="Nothing in flight" />
            <Section title="History" items={history} empty="No transfers yet" />
          </div>
        </motion.aside>
      ) : null}
    </AnimatePresence>
  );
}

function Section({
  title,
  items,
  empty,
}: {
  title: string;
  items: Transfer[];
  empty: string;
}) {
  return (
    <section className="transfers-section">
      <h4 className="transfers-section-title">{title}</h4>
      <div className="transfers-section-body">
        {items.length === 0 ? (
          <div className="transfers-empty">{empty}</div>
        ) : (
          <AnimatePresence>
            {items.map((t) => (
              <TransferRow key={t.id} transfer={t} />
            ))}
          </AnimatePresence>
        )}
      </div>
    </section>
  );
}

/**
 * "Right-now" throughput estimator. We sample (bytes, timestamp) on
 * each progress event and feed an exponential moving average (alpha
 * = 0.3) of `delta_bytes / delta_seconds` so the displayed number
 * tracks the *current* rate, not the cumulative average since the
 * transfer started. For terminal states we collapse to the simple
 * total-bytes / total-elapsed average so the row keeps showing a
 * sensible last value.
 */
function useLiveSpeed(transfer: Transfer): number {
  const [smoothed, setSmoothed] = useState(0);
  const sampleRef = useRef({ bytes: 0, ts: 0, ema: 0, initialised: false });

  useEffect(() => {
    if (!isActive(transfer)) {
      // Finished / cancelled / failed — show the overall average so
      // the row's final value is honest about how the transfer went.
      if (transfer.finished_at) {
        const start = new Date(transfer.started_at).getTime();
        const end = new Date(transfer.finished_at).getTime();
        const elapsed = Math.max(0.001, (end - start) / 1000);
        setSmoothed(transfer.bytes_done / elapsed);
      }
      return;
    }

    const now = Date.now();
    if (!sampleRef.current.initialised) {
      sampleRef.current = {
        bytes: transfer.bytes_done,
        ts: now,
        ema: 0,
        initialised: true,
      };
      return;
    }

    const elapsedSec = (now - sampleRef.current.ts) / 1000;
    // Need at least 100ms between samples to avoid wild numbers when
    // two progress events come back-to-back.
    if (elapsedSec < 0.1) return;

    const deltaBytes = transfer.bytes_done - sampleRef.current.bytes;
    if (deltaBytes < 0) {
      // Should not happen, but guard against a transfer-reset edge.
      sampleRef.current = {
        bytes: transfer.bytes_done,
        ts: now,
        ema: 0,
        initialised: true,
      };
      setSmoothed(0);
      return;
    }
    const instant = deltaBytes / elapsedSec;
    // First real sample seeds the EMA; subsequent ones blend.
    const alpha = sampleRef.current.ema === 0 ? 1 : 0.3;
    const ema = sampleRef.current.ema * (1 - alpha) + instant * alpha;

    sampleRef.current = { bytes: transfer.bytes_done, ts: now, ema, initialised: true };
    setSmoothed(ema);
  }, [
    transfer.bytes_done,
    transfer.status,
    transfer.started_at,
    transfer.finished_at,
  ]);

  return smoothed;
}

function TransferRow({ transfer }: { transfer: Transfer }) {
  const pct = formatPercent(transfer.bytes_done, transfer.total_bytes);
  const directionIcon =
    transfer.direction === "send" ? (
      <ArrowUpFromLine size={14} />
    ) : (
      <ArrowDownToLine size={14} />
    );
  const fileLine =
    transfer.files.length === 1
      ? transfer.files[0]!.name
      : `${transfer.files.length} files`;
  const sizeLine = `${formatBytes(transfer.bytes_done)} / ${formatBytes(transfer.total_bytes)}`;

  const speedBps = useLiveSpeed(transfer);

  const statusBadge = (() => {
    switch (transfer.status) {
      case "active":
        return (
          <span className="transfer-status status-active">
            <Loader2 size={11} className="spin" />
            transferring
          </span>
        );
      case "completed":
        return (
          <span className="transfer-status status-completed">
            <CheckCircle2 size={11} />
            done
          </span>
        );
      case "failed":
        return (
          <span className="transfer-status status-failed">
            <XCircle size={11} />
            failed
          </span>
        );
      case "cancelled":
        return <span className="transfer-status status-cancelled">cancelled</span>;
      case "rejected":
        return <span className="transfer-status status-cancelled">rejected</span>;
      case "awaiting-approval":
        return <span className="transfer-status status-awaiting">awaiting approval</span>;
      default:
        return <span className="transfer-status">{transfer.status}</span>;
    }
  })();

  const showCancel = isActive(transfer);

  return (
    <motion.div
      layout
      initial={{ opacity: 0, y: 6 }}
      animate={{ opacity: 1, y: 0 }}
      exit={{ opacity: 0, y: -6 }}
      transition={{ duration: 0.18 }}
      className="transfer-row"
    >
      <div className="transfer-row-head">
        <span className={`transfer-direction direction-${transfer.direction}`}>
          {directionIcon}
        </span>
        <span className="transfer-peer">{transfer.peer_name}</span>
        {statusBadge}
      </div>
      <div className="transfer-files" title={fileLine}>
        {fileLine}
      </div>
      <div className="transfer-progress-track">
        <div
          className={`transfer-progress-fill status-${transfer.status}`}
          style={{
            width:
              transfer.total_bytes > 0
                ? `${Math.min(100, (transfer.bytes_done / transfer.total_bytes) * 100)}%`
                : "0%",
          }}
        />
      </div>
      <div className="transfer-meta">
        <span>{sizeLine}</span>
        {speedBps > 0 ? (
          <span className="transfer-speed" title="Average throughput">
            {formatSpeed(speedBps)}
          </span>
        ) : null}
        <span>
          {pct}
          {showCancel ? (
            <button
              className="transfer-cancel"
              onClick={() => api.cancelTransfer(transfer.id)}
              title="Cancel"
            >
              <X size={11} />
            </button>
          ) : null}
        </span>
      </div>
      {transfer.error ? <div className="transfer-error">{transfer.error}</div> : null}
    </motion.div>
  );
}

import { AnimatePresence, motion } from "framer-motion";
import { ArrowDownToLine, Check, X } from "lucide-react";

import type { Transfer } from "../lib/tauri";
import { formatBytes } from "../lib/format";
import "./ReceivePrompt.css";

interface ReceivePromptProps {
  transfer: Transfer | null;
  onAccept: (id: string) => void;
  onReject: (id: string) => void;
}

export function ReceivePrompt({ transfer, onAccept, onReject }: ReceivePromptProps) {
  return (
    <AnimatePresence>
      {transfer ? (
        <motion.div
          key="rp-overlay"
          className="receive-prompt-overlay"
          initial={{ opacity: 0 }}
          animate={{ opacity: 1 }}
          exit={{ opacity: 0 }}
          transition={{ duration: 0.18 }}
        >
          <motion.div
            className="receive-prompt"
            initial={{ scale: 0.94, y: 16, opacity: 0 }}
            animate={{ scale: 1, y: 0, opacity: 1 }}
            exit={{ scale: 0.94, y: 16, opacity: 0 }}
            transition={{ type: "spring", stiffness: 280, damping: 24 }}
          >
            <div className="receive-prompt-icon">
              <ArrowDownToLine size={20} />
            </div>
            <div className="receive-prompt-title">
              <strong>{transfer.peer_name}</strong> wants to send you{" "}
              {transfer.files.length === 1
                ? "1 file"
                : `${transfer.files.length} files`}
            </div>
            <div className="receive-prompt-size">
              {formatBytes(transfer.total_bytes)} total
            </div>

            <ul className="receive-prompt-files">
              {transfer.files.slice(0, 5).map((f, i) => (
                <li key={`${f.name}-${i}`}>
                  <span className="rp-file-name">{f.name}</span>
                  <span className="rp-file-size">{formatBytes(f.size)}</span>
                </li>
              ))}
              {transfer.files.length > 5 ? (
                <li className="receive-prompt-more">
                  +{transfer.files.length - 5} more
                </li>
              ) : null}
            </ul>

            <div className="receive-prompt-actions">
              <button
                className="rp-btn rp-btn-secondary"
                onClick={() => onReject(transfer.id)}
              >
                <X size={14} />
                Reject
              </button>
              <button
                className="rp-btn rp-btn-primary"
                onClick={() => onAccept(transfer.id)}
                autoFocus
              >
                <Check size={14} />
                Accept
              </button>
            </div>
          </motion.div>
        </motion.div>
      ) : null}
    </AnimatePresence>
  );
}

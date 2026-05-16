import { useEffect, useState } from "react";
import { AnimatePresence, motion } from "framer-motion";
import { open as openDialog } from "@tauri-apps/plugin-dialog";
import { FolderOpen, X } from "lucide-react";
import { isEnabled, enable, disable } from "@tauri-apps/plugin-autostart";

import type { Settings } from "../../lib/tauri";
import { useSettingsStore } from "../../stores/settingsStore";
import { useToastStore } from "../../stores/toastStore";
import "./SettingsDialog.css";

interface SettingsDialogProps {
  open: boolean;
  onClose: () => void;
}

export function SettingsDialog({ open, onClose }: SettingsDialogProps) {
  const settings = useSettingsStore((s) => s.settings);
  const update = useSettingsStore((s) => s.update);
  const addToast = useToastStore((s) => s.addToast);

  const [draft, setDraft] = useState<Settings | null>(settings);
  const [autostartOn, setAutostartOn] = useState(false);
  const [saving, setSaving] = useState(false);

  useEffect(() => {
    setDraft(settings);
  }, [settings, open]);

  useEffect(() => {
    if (!open) return;
    isEnabled()
      .then((v) => setAutostartOn(!!v))
      .catch(() => setAutostartOn(false));
  }, [open]);

  if (!draft) return null;

  const handlePickDir = async () => {
    try {
      const dir = await openDialog({
        directory: true,
        multiple: false,
        title: "Choose download folder",
      });
      if (typeof dir === "string") {
        setDraft({ ...draft, download_dir: dir });
      }
    } catch (e) {
      addToast(`Could not open picker: ${e}`, "error");
    }
  };

  const handleSave = async () => {
    if (!draft) return;
    setSaving(true);
    try {
      // Autostart is managed via its dedicated plugin and not part of
      // the settings payload sent to Rust (settings.start_on_login is
      // kept as a UI mirror of the OS-level autostart state).
      if (draft.start_on_login !== autostartOn) {
        if (draft.start_on_login) {
          await enable();
        } else {
          await disable();
        }
      }
      await update(draft);
      addToast("Settings saved", "success");
      onClose();
    } catch (e) {
      addToast(`Save failed: ${e}`, "error");
    } finally {
      setSaving(false);
    }
  };

  return (
    <AnimatePresence>
      {open ? (
        <motion.div
          key="settings-overlay"
          className="settings-overlay"
          initial={{ opacity: 0 }}
          animate={{ opacity: 1 }}
          exit={{ opacity: 0 }}
          transition={{ duration: 0.16 }}
          onClick={onClose}
        >
          <motion.div
            className="settings-dialog"
            initial={{ scale: 0.96, y: 8, opacity: 0 }}
            animate={{ scale: 1, y: 0, opacity: 1 }}
            exit={{ scale: 0.96, y: 8, opacity: 0 }}
            transition={{ type: "spring", stiffness: 280, damping: 24 }}
            onClick={(e) => e.stopPropagation()}
          >
            <header className="settings-header">
              <h3>Settings</h3>
              <button className="settings-close" onClick={onClose}>
                <X size={16} />
              </button>
            </header>

            <div className="settings-body">
              <Field label="Display name" hint="What other devices see on the network.">
                <input
                  type="text"
                  value={draft.display_name}
                  onChange={(e) => setDraft({ ...draft, display_name: e.target.value })}
                  maxLength={48}
                />
              </Field>

              <Field
                label="Download folder"
                hint="Incoming files are saved here. Filename collisions get a numeric suffix automatically."
              >
                <div className="settings-row">
                  <input
                    type="text"
                    value={draft.download_dir}
                    onChange={(e) => setDraft({ ...draft, download_dir: e.target.value })}
                  />
                  <button className="settings-btn-icon" onClick={handlePickDir}>
                    <FolderOpen size={14} />
                  </button>
                </div>
              </Field>

              <Toggle
                label="Auto-accept incoming transfers"
                hint="Skip the 'Accept?' prompt. Useful between your own devices on a trusted network."
                value={draft.auto_accept}
                onChange={(v) => setDraft({ ...draft, auto_accept: v })}
              />

              <Toggle
                label="Start minimized to tray"
                hint="Launch without showing the main window; access from the tray icon."
                value={draft.start_minimized}
                onChange={(v) => setDraft({ ...draft, start_minimized: v })}
              />

              <Toggle
                label="Start on login"
                hint="Register Yonder with the OS so it runs in the background after login."
                value={draft.start_on_login}
                onChange={(v) => setDraft({ ...draft, start_on_login: v })}
              />
            </div>

            <footer className="settings-footer">
              <button className="rp-btn" onClick={onClose}>
                Cancel
              </button>
              <button
                className="rp-btn rp-btn-primary"
                onClick={handleSave}
                disabled={saving}
              >
                {saving ? "Saving…" : "Save"}
              </button>
            </footer>
          </motion.div>
        </motion.div>
      ) : null}
    </AnimatePresence>
  );
}

function Field({
  label,
  hint,
  children,
}: {
  label: string;
  hint?: string;
  children: React.ReactNode;
}) {
  return (
    <label className="settings-field">
      <div className="settings-field-label">{label}</div>
      {children}
      {hint ? <div className="settings-field-hint">{hint}</div> : null}
    </label>
  );
}

function Toggle({
  label,
  hint,
  value,
  onChange,
}: {
  label: string;
  hint?: string;
  value: boolean;
  onChange: (v: boolean) => void;
}) {
  return (
    <div className="settings-toggle">
      <div className="settings-toggle-text">
        <div className="settings-field-label">{label}</div>
        {hint ? <div className="settings-field-hint">{hint}</div> : null}
      </div>
      <button
        type="button"
        role="switch"
        aria-checked={value}
        className={`toggle ${value ? "on" : ""}`}
        onClick={() => onChange(!value)}
      >
        <span className="toggle-thumb" />
      </button>
    </div>
  );
}

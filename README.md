# Yonder

> Real-time LAN file sharing with a system tray and an AirDrop-style UI.
>
> Codename for the project; rename touchpoints are listed at the bottom of this file.

Yonder runs quietly in your system tray and advertises itself over **mDNS** so every other device running Yonder on the same Wi-Fi or LAN shows up in real time. Drag files onto a device card and they're sent over plain HTTP — no cloud, no accounts, no internet required.

- **Real-time discovery** &mdash; peers appear/disappear with a smooth animation as they come online / go offline. No refresh button needed.
- **System tray** &mdash; closing the window hides it; the tray icon brings it back. Optional "Start minimized" + "Start on login".
- **Drag and drop** &mdash; drop OS files onto a peer card to send. Also a file picker for multi-file selection.
- **Accept / reject prompt** &mdash; incoming transfers show sender + file list before they're written to disk. Auto-accept toggle for trusted networks.
- **Transfers panel** &mdash; active progress bars + history with cancel and clear.
- **Light + dark themes**.

Built with Tauri 2 + Rust on the back end and React 19 + TypeScript on the front end. Architecture and conventions follow the [Tablio](../tablio/) project layout.

---

## Quick start

```bash
cd ~/yonder
npm install                # one-time
npm run tauri dev          # hot-reload dev build
```

The first launch creates `~/.config/yonder/settings.json` with sensible defaults (display name = your hostname, download dir = `~/Downloads/Yonder`, port `53317`).

### Building a release

```bash
npm run tauri build
```

Bundles end up in `src-tauri/target/release/bundle/` (`.deb`, `.AppImage`, `.rpm`, etc.).

### System dependencies (Linux)

Same as Tablio:

```bash
sudo apt install libwebkit2gtk-4.1-dev libsoup-3.0-dev \
  libjavascriptcoregtk-4.1-dev librsvg2-dev
```

---

## How it works

```
┌─────────────────────────────────────────────────────────────────┐
│ Yonder app on device A                                          │
│  ┌─────────────────┐    ┌───────────────────┐                   │
│  │   React UI      │◀──▶│  Tauri IPC + emit │                   │
│  └─────────────────┘    └────────┬──────────┘                   │
│           ▲                       │                             │
│           │ peer events           ▼                             │
│  ┌────────┴──────────┐  ┌──────────────────┐  ┌──────────────┐  │
│  │ mDNS daemon       │  │ axum HTTP server │  │ reqwest send │  │
│  │ advertise + browse│  │ POST /upload     │  │ POST /upload │  │
│  └─────────┬─────────┘  └──────────────────┘  └──────┬───────┘  │
└────────────┼────────────────────▲──────────────────-─┼──────────┘
             │                    │                    │
       multicast 224.0.0.251     plain HTTP over the LAN
             │                    │                    ▼
                       Yonder app on device B
```

- **Discovery** is `mdns-sd` advertising the service type `_yonder._tcp.local.` with TXT records (`id`, `name`, `os`, `v`). Each peer also browses the same type and pushes `ServiceResolved` / `ServiceRemoved` events back to the frontend as `peer-added` / `peer-removed`. No polling, no manual refresh.
- **Transfer** is plain HTTP (no TLS, no internet round-trip):
  - `GET /info` returns the device identity (used as a pre-flight check before a send).
  - `POST /upload?session=<uuid>&sender=<id>&sender_name=<name>` accepts a `multipart/form-data` body whose **first** part is a `meta` JSON field (`{ files: [{name, size, mime}] }`) and whose remaining parts are the actual files. The server reads `meta` first so it can prompt the user before allocating disk.
  - Progress events are throttled to ~10 Hz per transfer to keep the IPC bridge light.
- **Approval flow** parks the axum handler on a `oneshot::Sender<ApprovalDecision>` registered in `AppState`. The frontend's `accept_incoming` / `reject_incoming` commands resolve it. If `auto_accept` is on, the gate is skipped entirely.

### Project layout

```
yonder/
├── package.json
├── vite.config.ts
├── index.html
├── src/                            # React frontend
│   ├── App.tsx                     # main layout + IPC wiring
│   ├── components/
│   │   ├── TitleBar/               # custom title bar (decorations: false)
│   │   ├── PeerGrid/               # animated grid of device cards
│   │   ├── TransfersPanel/         # active + history rows
│   │   ├── SettingsDialog/         # name, port, download dir, …
│   │   ├── ReceivePrompt.tsx       # accept/reject modal
│   │   └── Toast/
│   ├── stores/                     # Zustand (peers / transfers / settings / toasts)
│   ├── lib/
│   │   ├── tauri.ts                # invoke + listen wrappers (one source of truth)
│   │   └── format.ts               # byte / percent / monogram helpers
│   └── styles/global.css           # theme tokens (data-theme="dark|light")
└── src-tauri/
    ├── Cargo.toml
    ├── tauri.conf.json             # decorations:false, 1000x720 window
    ├── capabilities/default.json   # fs / dialog / window / tray / autostart
    └── src/
        ├── lib.rs                  # builder + tray + invoke_handler
        ├── state.rs                # AppState (peers, transfers, settings)
        ├── config.rs               # ~/.config/yonder/settings.json
        ├── identity.rs             # uuid + hostname + os tag
        ├── discovery.rs            # mdns-sd advertise + browse
        ├── server.rs               # axum receive server
        ├── client.rs               # reqwest multipart sender
        ├── transfer.rs             # Transfer / Direction / TransferStatus
        └── commands/               # Tauri IPC: peers / transfers / settings / window
```

## Keyboard shortcuts

| Shortcut          | Action                |
|-------------------|-----------------------|
| `Esc`             | Close modal / panel   |
| `Ctrl` + `,`      | Open settings         |
| `Ctrl` + `T`      | Toggle transfers      |

## Settings

Persisted at `~/.config/yonder/settings.json`:

| Field             | Default                  | Notes                                                              |
|-------------------|--------------------------|--------------------------------------------------------------------|
| `device_id`       | random UUID              | Stable; do not edit by hand.                                       |
| `display_name`    | OS hostname              | Updates the mDNS TXT record on save.                               |
| `download_dir`    | `~/Downloads/Yonder`     | Created on first run; collisions auto-suffixed `name (2).ext`.     |
| `tcp_port`        | `53317`                  | TCP port for the receive server; also advertised over mDNS.        |
| `auto_accept`     | `false`                  | Skip the "Accept?" prompt for all incoming transfers.              |
| `start_minimized` | `false`                  | Hide the main window at launch (tray-only).                        |
| `start_on_login`  | `false`                  | Register Yonder with the OS autostart system.                      |
| `theme`           | `"dark"`                 | `"dark"` or `"light"`.                                              |

## Limitations (v1)

- **Plain HTTP only**. No TLS / pairing PIN. Use only on trusted networks.
- **No resumable transfers**. A dropped connection means the receiver gets a partial file you can delete; the sender will surface the error and you can re-send.
- **Cancel is soft**: marking a transfer cancelled does not yet abort the in-flight reqwest stream. Closing the app stops everything.
- **Desktop only**. Mobile clients can be added later — the protocol is just HTTP, so a small Android/iOS app could speak it.

## Renaming the codename

This project ships as **Yonder** as a working codename. To rename, run a case-sensitive search/replace across:

- `package.json` → `"name"`
- `src-tauri/Cargo.toml` → `[package].name`, `[lib].name`
- `src-tauri/tauri.conf.json` → `productName`, `identifier`, window `title`
- `src-tauri/src/discovery.rs` → `SERVICE_TYPE` constant (`_yonder._tcp.local.`)
- `src-tauri/src/config.rs` → `config_dir()` (`yonder` directory)
- `src-tauri/src/lib.rs` → tray menu labels / tooltip
- `src/components/TitleBar/TitleBar.tsx` → title text
- `index.html` → `<title>`, `localStorage` key (`yonder-theme`)
- `README.md` → this file

## License

MIT

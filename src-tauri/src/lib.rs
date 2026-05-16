pub mod client;
pub mod commands;
pub mod config;
pub mod discovery;
pub mod identity;
pub mod server;
pub mod state;
pub mod transfer;

use std::sync::Mutex;

use tauri::{
    menu::{MenuBuilder, MenuItemBuilder},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    Manager, WindowEvent,
};

use commands::*;
use discovery::Discovery;
use identity::Identity;
use state::AppState;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let settings = config::load_or_init();
    let identity = Identity::new(
        settings.device_id.clone(),
        Some(settings.display_name.clone()),
    );
    let app_state = AppState::new(settings.clone(), identity);

    tauri::Builder::default()
        .plugin(tauri_plugin_fs::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            None,
        ))
        .manage(app_state.clone())
        .manage::<Mutex<Option<Discovery>>>(Mutex::new(None))
        .setup(move |app| {
            // ── Tray with Show / Hide / Quit -----------------------------------
            let show_item = MenuItemBuilder::with_id("show", "Show Yonder").build(app)?;
            let hide_item = MenuItemBuilder::with_id("hide", "Hide window").build(app)?;
            let quit_item = MenuItemBuilder::with_id("quit", "Quit Yonder").build(app)?;
            let tray_menu = MenuBuilder::new(app)
                .items(&[&show_item, &hide_item])
                .separator()
                .items(&[&quit_item])
                .build()?;

            let _tray = TrayIconBuilder::with_id("main")
                .icon(app.default_window_icon().unwrap().clone())
                .menu(&tray_menu)
                .show_menu_on_left_click(false)
                .tooltip("Yonder — file sharing on your LAN")
                .on_menu_event(|app, event| match event.id.as_ref() {
                    "show" => {
                        if let Some(w) = app.get_webview_window("main") {
                            let _ = w.show();
                            let _ = w.unminimize();
                            let _ = w.set_focus();
                        }
                    }
                    "hide" => {
                        if let Some(w) = app.get_webview_window("main") {
                            let _ = w.hide();
                        }
                    }
                    "quit" => app.exit(0),
                    _ => {}
                })
                .on_tray_icon_event(|tray, event| {
                    if let TrayIconEvent::Click {
                        button: MouseButton::Left,
                        button_state: MouseButtonState::Up,
                        ..
                    } = event
                    {
                        let app = tray.app_handle();
                        if let Some(w) = app.get_webview_window("main") {
                            // Toggle: if visible, hide; otherwise show + focus.
                            let visible = w.is_visible().unwrap_or(false);
                            if visible {
                                let _ = w.hide();
                            } else {
                                let _ = w.show();
                                let _ = w.unminimize();
                                let _ = w.set_focus();
                            }
                        }
                    }
                })
                .build(app)?;

            // Honour the saved "start minimized" preference.
            if settings.start_minimized {
                if let Some(w) = app.get_webview_window("main") {
                    let _ = w.hide();
                }
            }

            // ── Spin up HTTP receive server + mDNS discovery ------------------
            let handle = app.handle().clone();
            let state_for_server = app_state.clone();
            let port = settings.tcp_port;

            tauri::async_runtime::spawn(async move {
                match server::spawn(handle.clone(), state_for_server, port).await {
                    Ok(bound) => {
                        log::info!("HTTP server bound to {bound}");
                        match Discovery::start(handle.clone(), bound.port()) {
                            Ok(discovery) => {
                                let slot = handle.state::<Mutex<Option<Discovery>>>();
                                let mut guard = slot.lock().expect("discovery slot poisoned");
                                *guard = Some(discovery);
                            }
                            Err(e) => {
                                log::error!("mDNS discovery failed to start: {e}");
                            }
                        }
                    }
                    Err(e) => {
                        log::error!("HTTP server failed to start: {e}");
                    }
                }
            });

            Ok(())
        })
        .on_window_event(|window, event| {
            // Intercept the user closing the window: hide it instead of
            // exiting so the app keeps running in the tray.
            if let WindowEvent::CloseRequested { api, .. } = event {
                if window.label() == "main" {
                    let _ = window.hide();
                    api.prevent_close();
                }
            }
        })
        .invoke_handler(tauri::generate_handler![
            list_peers,
            get_self,
            list_transfers,
            send_files,
            accept_incoming,
            reject_incoming,
            cancel_transfer,
            get_settings,
            update_settings,
            show_main,
            hide_main,
            quit_app,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

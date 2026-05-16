pub mod accept;
pub mod client;
pub mod commands;
pub mod config;
pub mod identity;
pub mod net;
pub mod state;
pub mod transfer;

use tauri::{
    menu::{MenuBuilder, MenuItemBuilder},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    Manager, WindowEvent,
};

use commands::*;
use identity::Identity;
use state::AppState;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let settings = config::load_or_init();
    let secret_key = settings
        .secret()
        .expect("settings.json secret_key parsed at load time");
    let identity = Identity::new(&secret_key, Some(settings.display_name.clone()));
    let app_state = AppState::new(settings.clone(), identity.clone());

    tauri::Builder::default()
        .plugin(tauri_plugin_fs::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            None,
        ))
        .manage(app_state.clone())
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

            if settings.start_minimized {
                if let Some(w) = app.get_webview_window("main") {
                    let _ = w.hide();
                }
            }

            // ── Spin up the iroh endpoint + accept loop + discovery loop -----
            let handle = app.handle().clone();
            let app_state_for_spawn = app_state.clone();
            let identity_for_spawn = identity.clone();

            tauri::async_runtime::spawn(async move {
                let user_data = net::PeerUserDataIn {
                    name: identity_for_spawn.name.clone(),
                    os: identity_for_spawn.os.clone(),
                    version: identity_for_spawn.version.clone(),
                };
                match net::build_endpoint(secret_key, user_data).await {
                    Ok((endpoint, mdns)) => {
                        log::info!("iroh endpoint ready: {}", endpoint.id());
                        // Make the endpoint reachable from Tauri commands
                        // (send_files, update_settings).
                        handle.manage(endpoint.clone());

                        // Run accept + discovery concurrently for the
                        // lifetime of the app. They each return when the
                        // endpoint is closed.
                        let accept_app = handle.clone();
                        let accept_state = app_state_for_spawn.clone();
                        let accept_endpoint = endpoint.clone();
                        tokio::spawn(async move {
                            accept::run_accept_loop(accept_endpoint, accept_state, accept_app)
                                .await;
                        });

                        let disc_app = handle.clone();
                        let disc_state = app_state_for_spawn.clone();
                        tokio::spawn(async move {
                            net::run_discovery_loop(disc_app, disc_state, mdns).await;
                        });
                    }
                    Err(e) => {
                        log::error!("could not start iroh endpoint: {e:#}");
                    }
                }
            });

            Ok(())
        })
        .on_window_event(|window, event| {
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

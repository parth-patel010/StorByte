mod auth;
mod automount;
mod autostart;
mod blocks;
mod cache;
mod commands;
mod config;
mod export;
mod filesystem;
mod harden;
mod install_reg;
mod metadata;
mod prefetch;
mod providers;
mod scheduler;
mod state;
mod stream_seal;

use state::AppState;
use std::env;
use std::path::PathBuf;
use tauri::menu::{Menu, MenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{Manager, WindowEvent};
use tracing_subscriber::EnvFilter;

/// If Storbyte is already running (tray), focus that window and exit this process
/// so we do not remount / fight over the drive letter.
#[cfg(windows)]
fn focus_existing_or_claim_instance() -> bool {
    use windows::core::PCWSTR;
    use windows::Win32::Foundation::{CloseHandle, ERROR_ALREADY_EXISTS, GetLastError, HWND};
    use windows::Win32::System::Threading::CreateMutexW;
    use windows::Win32::UI::WindowsAndMessaging::{
        FindWindowW, IsIconic, SetForegroundWindow, ShowWindow, SW_RESTORE,
    };

    let name: Vec<u16> = "Local\\StorbyteDesktopSingleInstance\0"
        .encode_utf16()
        .collect();
    let handle = unsafe { CreateMutexW(None, true, PCWSTR(name.as_ptr())) };
    let already = unsafe { GetLastError() } == ERROR_ALREADY_EXISTS;
    if let Ok(h) = handle {
        if already {
            let _ = unsafe { CloseHandle(h) };
            let title = wide_null("Storbyte");
            let hwnd = unsafe { FindWindowW(PCWSTR::null(), PCWSTR(title.as_ptr())) }
                .unwrap_or(HWND::default());
            if !hwnd.is_invalid() && hwnd != HWND::default() {
                unsafe {
                    if IsIconic(hwnd).as_bool() {
                        let _ = ShowWindow(hwnd, SW_RESTORE);
                    }
                    let _ = SetForegroundWindow(hwnd);
                }
            }
            return false; // do not start second app
        }
        // Keep mutex open for process lifetime (do not CloseHandle).
        let _hold = h;
        return true;
    }
    true
}

#[cfg(windows)]
fn wide_null(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

#[cfg(not(windows))]
fn focus_existing_or_claim_instance() -> bool {
    true
}

fn load_dotenv() {
    // Optional overrides only — release builds embed Google OAuth defaults at compile time.
    let mut candidates = vec![
        PathBuf::from(".env"),
        PathBuf::from("../.env"),
        PathBuf::from("../../.env"),
    ];
    if let Ok(exe) = env::current_exe() {
        if let Some(dir) = exe.parent() {
            candidates.insert(0, dir.join(".env"));
        }
    }
    for p in candidates {
        if p.exists() {
            if let Ok(raw) = std::fs::read_to_string(&p) {
                for line in raw.lines() {
                    let line = line.trim();
                    if line.is_empty() || line.starts_with('#') {
                        continue;
                    }
                    if let Some((k, v)) = line.split_once('=') {
                        if env::var(k).is_err() {
                            env::set_var(k.trim(), v.trim());
                        }
                    }
                }
            }
            break;
        }
    }
}

fn handle_cli() -> bool {
    let args: Vec<String> = env::args().collect();
    if args.iter().any(|a| a == "--uninstall") {
        let quiet = args.iter().any(|a| a == "--quiet");
        if let Err(e) = install_reg::unregister_installed_app() {
            eprintln!("uninstall registry cleanup failed: {e}");
        }
        if !quiet {
            eprintln!("Storbyte was removed from Installed apps / Startup.");
            eprintln!("You can delete Storbyte.exe manually if you no longer need it.");
        }
        return true;
    }
    if args.len() >= 3 && args[1] == "--open-link" {
        load_dotenv();
        let _ = tracing_subscriber::fmt()
            .with_env_filter(EnvFilter::from_default_env().add_directive("info".parse().unwrap()))
            .try_init();
        if let Ok(db) = metadata::MetadataDb::open_default() {
            if let Err(e) = export::open_link(&db, &PathBuf::from(&args[2])) {
                eprintln!("open-link failed: {e}");
            }
        }
        return true;
    }
    if args.len() >= 3 && args[1] == "--export-link" {
        load_dotenv();
        let _ = tracing_subscriber::fmt()
            .with_env_filter(EnvFilter::from_default_env().add_directive("info".parse().unwrap()))
            .try_init();
        if let Err(e) = export::export_link_cli(&args[2]) {
            eprintln!("export-link failed: {e}");
        }
        return true;
    }
    false
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    load_dotenv();
    if handle_cli() {
        return;
    }

    if !focus_existing_or_claim_instance() {
        return;
    }

    harden::apply_release_hardening();

    // Release: keep logs quiet so internals are not dumped to consoles/log files.
    #[cfg(debug_assertions)]
    let filter = EnvFilter::from_default_env().add_directive("info".parse().unwrap());
    #[cfg(not(debug_assertions))]
    let filter = EnvFilter::from_default_env().add_directive("warn".parse().unwrap());

    tracing_subscriber::fmt().with_env_filter(filter).init();

    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .expect("tokio runtime");
    let handle = runtime.handle().clone();

    // Keep runtime alive for the process lifetime.
    std::mem::forget(runtime);

    let app_state = AppState::new(handle).expect("init app state");

    if let Ok(exe) = env::current_exe() {
        if let Err(e) = export::register_shell_integration(&exe) {
            tracing::warn!(error = %e, "shell integration registration failed");
        }
        if let Err(e) = install_reg::register_installed_app() {
            tracing::warn!(error = %e, "installed-apps registration failed");
        }
    }

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_shell::init())
        .manage(app_state)
        .setup(|app| {
            let state = app.state::<AppState>();
            state.start_change_watcher(app.handle().clone());

            // Sync Windows Run key with saved preference (default: on).
            let launch = state.settings.read().launch_at_startup;
            if let Err(e) = crate::autostart::set_launch_at_startup(launch) {
                tracing::warn!(error = %e, "could not update launch-at-startup");
            }

            // Login autostart: stay in tray until the user opens the window.
            let from_startup = env::args().any(|a| a == "--autostart");
            if from_startup {
                if let Some(w) = app.get_webview_window("main") {
                    let _ = w.hide();
                }
            }

            crate::automount::spawn(app.handle().clone());

            // Tray so closing the window can leave V: mounted in the background.
            let show_i = MenuItem::with_id(app, "show", "Open Storbyte", true, None::<&str>)?;
            let quit_i = MenuItem::with_id(app, "quit", "Quit Storbyte", true, None::<&str>)?;
            let menu = Menu::with_items(app, &[&show_i, &quit_i])?;
            let icon = app
                .default_window_icon()
                .cloned()
                .expect("missing window icon");
            let _tray = TrayIconBuilder::new()
                .icon(icon)
                .menu(&menu)
                .tooltip("Storbyte")
                .on_menu_event(|app, event| match event.id.as_ref() {
                    "show" => {
                        if let Some(w) = app.get_webview_window("main") {
                            let _ = w.show();
                            let _ = w.set_focus();
                        }
                    }
                    "quit" => {
                        let state = app.state::<AppState>();
                        let _ = state.unmount_drive();
                        app.exit(0);
                    }
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
                            let _ = w.show();
                            let _ = w.set_focus();
                        }
                    }
                })
                .build(app)?;

            Ok(())
        })
        .on_window_event(|window, event| {
            if let WindowEvent::CloseRequested { api, .. } = event {
                // Closing the window must not kill the WinFsp host — V: stays mounted.
                // Fully exit only via tray → Quit Storbyte (unmounts first).
                api.prevent_close();
                let _ = window.hide();
            }
        })
        .invoke_handler(tauri::generate_handler![
            commands::get_app_state,
            commands::get_settings,
            commands::update_settings,
            commands::list_accounts,
            commands::connect_google_account,
            commands::cancel_google_oauth,
            commands::disconnect_account,
            commands::remove_account,
            commands::reconnect_account,
            commands::sync_metadata,
            commands::sync_metadata_shallow,
            commands::ensure_directory,
            commands::read_file_preview,
            commands::create_storbyte_link_desktop,
            commands::mount_drive,
            commands::resolve_upload_account,
            commands::cancel_upload_account_pick,
            commands::upload_local_paths,
            commands::unmount_drive,
            commands::list_directory,
            commands::search_nodes,
            commands::refresh_account_quotas,
            commands::create_storbyte_link,
            commands::open_storbyte_link,
            commands::available_drive_letters,
            commands::winfsp_installed,
            commands::install_winfsp,
            commands::open_winfsp_installer,
        ])
        .run(tauri::generate_context!())
        .expect("error while running Storbyte");
}

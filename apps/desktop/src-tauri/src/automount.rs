//! Auto-mount Storbyte drive on launch (startup / tray) without a manual click.

use crate::filesystem::DriveStatus;
use crate::state::AppState;
use std::time::Duration;
use tauri::{AppHandle, Emitter, Manager};

/// Background loop: mount as soon as accounts + local index are ready.
/// Does **not** require network when SQLite already has an index (same as manual Mount).
pub fn spawn(app: AppHandle) {
    let runtime = app.state::<AppState>().runtime.clone();
    runtime.spawn(async move {
        // Short settle so tray/WinFsp are ready after login.
        tokio::time::sleep(Duration::from_millis(800)).await;
        let mut backoff = Duration::from_secs(2);
        loop {
            let should_run = {
                let state = app.state::<AppState>();
                let enabled = state.settings.read().auto_mount_on_launch;
                enabled
            };

            if !should_run {
                set_waiting(&app, false);
                tokio::time::sleep(Duration::from_secs(5)).await;
                continue;
            }

            {
                let state = app.state::<AppState>();
                if state.mount.status().mounted {
                    set_waiting(&app, false);
                    backoff = Duration::from_secs(2);
                    tokio::time::sleep(Duration::from_secs(12)).await;
                    continue;
                }

                let _ = state.reload_providers();
                if state.providers.read().is_empty() {
                    set_waiting(&app, false);
                    tokio::time::sleep(Duration::from_secs(4)).await;
                    continue;
                }

                let indexed = state.db.any_account_indexed().unwrap_or(false);
                let node_count = state.db.count_nodes().unwrap_or(0);
                if !indexed && node_count <= 1 {
                    // No local catalog yet — wait for sync (needs network).
                    if !network_reachable().await {
                        set_waiting(&app, true);
                        emit_status(&app);
                        tokio::time::sleep(backoff).await;
                        backoff = (backoff * 2).min(Duration::from_secs(15));
                        continue;
                    }
                    set_waiting(&app, false);
                    tokio::time::sleep(Duration::from_secs(5)).await;
                    continue;
                }
            }

            // Local index exists → mount immediately (manual Mount never waits on network).
            set_waiting(&app, false);
            let mount_result = {
                let state = app.state::<AppState>();
                state.mount_drive(app.clone())
            };
            match mount_result {
                Ok(st) => {
                    tracing::info!(letter = %st.drive_letter, "auto-mounted");
                    let _ = app.emit("drive-status", &st);
                    let _ = app.emit("app-state-changed", ());
                    backoff = Duration::from_secs(2);
                    tokio::time::sleep(Duration::from_secs(15)).await;
                }
                Err(e) => {
                    let msg = e.to_string();
                    tracing::warn!(error = %msg, "auto-mount failed; will retry");
                    {
                        let state = app.state::<AppState>();
                        state.mount.set_error(Some(msg));
                    }
                    emit_status(&app);
                    tokio::time::sleep(backoff).await;
                    backoff = (backoff * 2).min(Duration::from_secs(20));
                }
            }
        }
    });
}

fn set_waiting(app: &AppHandle, waiting: bool) {
    let state = app.state::<AppState>();
    state.mount.set_waiting_for_network(waiting);
}

fn emit_status(app: &AppHandle) {
    let state = app.state::<AppState>();
    let st: DriveStatus = state.mount.status();
    let _ = app.emit("drive-status", &st);
}

async fn network_reachable() -> bool {
    let targets = ["www.googleapis.com:443", "oauth2.googleapis.com:443", "8.8.8.8:53"];
    for addr in targets {
        let ok = tokio::time::timeout(
            Duration::from_secs(2),
            tokio::net::TcpStream::connect(addr),
        )
        .await
        .map(|r| r.is_ok())
        .unwrap_or(false);
        if ok {
            return true;
        }
    }
    false
}

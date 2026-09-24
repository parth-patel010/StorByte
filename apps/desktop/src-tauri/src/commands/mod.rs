use crate::config::SettingsPatch;
use crate::state::{AccountInfo, AppState, AppStateDto};
use crate::filesystem::DriveStatus;
use crate::config::AppSettings;
use crate::metadata::SyncProgress;
use tauri::{AppHandle, Emitter, State};

fn map_err(e: impl ToString) -> String {
    e.to_string()
}

#[tauri::command]
pub fn get_app_state(state: State<'_, AppState>) -> AppStateDto {
    state.dto()
}

#[tauri::command]
pub fn get_settings(state: State<'_, AppState>) -> AppSettings {
    state.settings.read().clone()
}

#[tauri::command]
pub fn update_settings(
    state: State<'_, AppState>,
    patch: SettingsPatch,
) -> Result<AppSettings, String> {
    state.update_settings(patch).map_err(map_err)
}

#[tauri::command]
pub fn list_accounts(state: State<'_, AppState>) -> Result<Vec<AccountInfo>, String> {
    Ok(state.dto().accounts)
}

#[tauri::command]
pub async fn connect_google_account(
    state: State<'_, AppState>,
) -> Result<AccountInfo, String> {
    state.connect_google().await.map_err(map_err)
}

#[tauri::command]
pub fn cancel_google_oauth(state: State<'_, AppState>) -> Result<(), String> {
    state.cancel_google_oauth();
    Ok(())
}

#[tauri::command]
pub fn disconnect_account(state: State<'_, AppState>, account_id: String) -> Result<(), String> {
    // Treat Settings "Sign Out" as a full clear so Connected accounts cannot show stale data.
    state.sign_out(&account_id).map_err(map_err)
}

#[tauri::command]
pub fn remove_account(state: State<'_, AppState>, account_id: String) -> Result<(), String> {
    state.remove_account(&account_id).map_err(map_err)
}

#[tauri::command]
pub async fn reconnect_account(
    state: State<'_, AppState>,
    account_id: String,
) -> Result<AccountInfo, String> {
    state.reconnect_account(&account_id).await.map_err(map_err)
}

#[tauri::command]
pub async fn sync_metadata(app: AppHandle, state: State<'_, AppState>) -> Result<(), String> {
    state
        .sync_all(|progress: SyncProgress| {
            let _ = app.emit("sync-progress", &progress);
        })
        .await
        .map_err(map_err)
}

#[tauri::command]
pub async fn sync_metadata_shallow(app: AppHandle, state: State<'_, AppState>) -> Result<(), String> {
    state
        .sync_all_shallow(|progress: SyncProgress| {
            let _ = app.emit("sync-progress", &progress);
        })
        .await
        .map_err(map_err)
}

#[tauri::command]
pub async fn ensure_directory(
    state: State<'_, AppState>,
    path: Option<String>,
) -> Result<u64, String> {
    let parent = path.unwrap_or_else(|| "\\".into());
    state.ensure_folder_children(&parent).await.map_err(map_err)
}

#[tauri::command]
pub fn read_file_preview(
    state: State<'_, AppState>,
    virtual_path: String,
    max_bytes: Option<u64>,
) -> Result<crate::state::FilePreviewDto, String> {
    let cap = max_bytes.unwrap_or(25 * 1024 * 1024);
    state.read_file_preview(&virtual_path, cap).map_err(map_err)
}

#[tauri::command]
pub fn create_storbyte_link_desktop(
    state: State<'_, AppState>,
    virtual_path: String,
) -> Result<String, String> {
    state.create_link_to_desktop(&virtual_path).map_err(map_err)
}

#[tauri::command]
pub fn mount_drive(app: AppHandle, state: State<'_, AppState>) -> Result<DriveStatus, String> {
    state.mount_drive(app).map_err(map_err)
}

#[tauri::command]
pub fn resolve_upload_account(
    state: State<'_, AppState>,
    account_id: String,
) -> Result<bool, String> {
    Ok(state.resolve_upload_account(account_id))
}

#[tauri::command]
pub fn cancel_upload_account_pick(state: State<'_, AppState>) -> Result<bool, String> {
    Ok(state.cancel_upload_account_pick())
}

#[tauri::command]
pub async fn upload_local_paths(
    app: AppHandle,
    state: State<'_, AppState>,
    parent_path: Option<String>,
    paths: Vec<String>,
) -> Result<u32, String> {
    let parent = parent_path.unwrap_or_else(|| "\\".into());
    state
        .upload_local_paths(app, &parent, paths)
        .await
        .map_err(map_err)
}

#[tauri::command]
pub fn unmount_drive(state: State<'_, AppState>) -> DriveStatus {
    state.unmount_drive()
}

#[tauri::command]
pub async fn refresh_account_quotas(
    state: State<'_, AppState>,
) -> Result<Vec<AccountInfo>, String> {
    state.refresh_all_quotas().await.map_err(map_err)
}

#[tauri::command]
pub fn search_nodes(
    state: State<'_, AppState>,
    query: String,
) -> Result<Vec<crate::metadata::NodeRow>, String> {
    state.db.search_nodes(&query, 200).map_err(map_err)
}

#[tauri::command]
pub fn list_directory(
    state: State<'_, AppState>,
    path: Option<String>,
) -> Result<Vec<crate::metadata::NodeRow>, String> {
    let parent = path.unwrap_or_else(|| "\\".into());
    state.db.list_children(&parent).map_err(map_err)
}

#[tauri::command]
pub fn create_storbyte_link(
    state: State<'_, AppState>,
    virtual_path: String,
    dest_dir: String,
) -> Result<String, String> {
    state.create_link(&virtual_path, &dest_dir).map_err(map_err)
}

#[tauri::command]
pub fn open_storbyte_link(state: State<'_, AppState>, path: String) -> Result<(), String> {
    state.open_link(&path).map_err(map_err)
}

#[tauri::command]
pub fn available_drive_letters() -> Vec<String> {
    #[cfg(windows)]
    {
        use windows::Win32::Storage::FileSystem::GetLogicalDrives;
        let mask = unsafe { GetLogicalDrives() };
        let preferred = ['V', 'W', 'X', 'Y', 'Z'];
        let mut out = Vec::new();
        for ch in preferred {
            let bit = 1u32 << (ch as u32 - 'A' as u32);
            if mask & bit == 0 {
                out.push(ch.to_string());
            }
        }
        // Also include other free letters as fallback
        for ch in b'A'..=b'Z' {
            let c = ch as char;
            if preferred.contains(&c) {
                continue;
            }
            let bit = 1u32 << (ch - b'A') as u32;
            if mask & bit == 0 {
                out.push(c.to_string());
            }
        }
        if out.is_empty() {
            out.push("V".into());
        }
        out
    }
    #[cfg(not(windows))]
    {
        vec!["V".into(), "W".into(), "X".into(), "Y".into(), "Z".into()]
    }
}

/// True when WinFsp DLL is present (needed for mounting / fast prefetch reads).
#[tauri::command]
pub fn winfsp_installed() -> bool {
    winfsp_dll_present()
}

fn winfsp_dll_present() -> bool {
    #[cfg(windows)]
    {
        use std::path::Path;
        [
            r"C:\Program Files (x86)\WinFsp\bin\winfsp-x64.dll",
            r"C:\Program Files\WinFsp\bin\winfsp-x64.dll",
        ]
        .iter()
        .any(|p| Path::new(p).exists())
    }
    #[cfg(not(windows))]
    {
        false
    }
}

/// Download/install WinFsp (elevated) and verify the DLL is present.
#[tauri::command]
pub async fn install_winfsp() -> Result<bool, String> {
    if winfsp_dll_present() {
        return Ok(true);
    }

    #[cfg(windows)]
    {
        // 1) Prefer winget (quiet + elevated UAC prompt)
        if install_winfsp_via_winget() {
            if winfsp_dll_present() {
                return Ok(true);
            }
        }

        // 2) Fallback: download official MSI and run elevated msiexec
        install_winfsp_via_msi().await?;
        if winfsp_dll_present() {
            return Ok(true);
        }

        return Err(
            "Install finished but the dependency was not detected yet. \
             Finish the installer (accept UAC if shown), then click Install again — \
             or restart Storbyte."
                .into(),
        );
    }

    #[cfg(not(windows))]
    {
        Err("This dependency is only available on Windows.".into())
    }
}

#[cfg(windows)]
fn install_winfsp_via_winget() -> bool {
    use std::os::windows::process::CommandExt;
    use std::process::Command;

    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    let args =
        "install -e --id WinFsp.WinFsp --silent --accept-package-agreements --accept-source-agreements";

    // Elevate via PowerShell so the user gets a normal UAC prompt.
    let ps = format!(
        "Start-Process -FilePath 'winget' -ArgumentList '{args}' -Verb RunAs -Wait -WindowStyle Hidden"
    );
    let status = Command::new("powershell")
        .args(["-NoProfile", "-ExecutionPolicy", "Bypass", "-Command", &ps])
        .creation_flags(CREATE_NO_WINDOW)
        .status();

    matches!(status, Ok(s) if s.success())
}

#[cfg(windows)]
async fn install_winfsp_via_msi() -> Result<(), String> {
    use std::os::windows::process::CommandExt;
    use std::process::Command;

    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    const MSI_URL: &str =
        "https://github.com/winfsp/winfsp/releases/download/v2.1/winfsp-2.1.25156.msi";

    let dest = std::env::temp_dir().join("Storbyte-winfsp-setup.msi");

    let bytes = reqwest::Client::new()
        .get(MSI_URL)
        .send()
        .await
        .map_err(|e| format!("Download failed: {e}"))?
        .error_for_status()
        .map_err(|e| format!("Download failed: {e}"))?
        .bytes()
        .await
        .map_err(|e| format!("Download failed: {e}"))?;

    std::fs::write(&dest, &bytes).map_err(|e| format!("Could not save installer: {e}"))?;

    let msi_path = dest.to_string_lossy().replace('\'', "''");
    let ps = format!(
        "Start-Process -FilePath 'msiexec.exe' -ArgumentList @('/i','{msi_path}','/qn','/norestart') -Verb RunAs -Wait -WindowStyle Hidden"
    );

    let status = Command::new("powershell")
        .args(["-NoProfile", "-ExecutionPolicy", "Bypass", "-Command", &ps])
        .creation_flags(CREATE_NO_WINDOW)
        .status()
        .map_err(|e| format!("Failed to start installer: {e}"))?;

    if !status.success() {
        // User may have cancelled UAC — still report clearly.
        return Err(
            "Installer did not complete. Accept the Windows security prompt and try again.".into(),
        );
    }
    Ok(())
}

/// Open the WinFsp installer download page (manual fallback).
#[tauri::command]
pub fn open_winfsp_installer() -> Result<(), String> {
    open::that("https://winfsp.dev/rel/")
        .map_err(|e| format!("Failed to open download page: {e}"))
}

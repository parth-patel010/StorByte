//! Register Storbyte in the current-user Windows Run key for login autostart,
//! and mark it Enabled in StartupApproved so it appears in Settings → Startup apps.

use anyhow::{anyhow, Context, Result};
use std::path::PathBuf;

#[cfg(windows)]
const RUN_SUBKEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";
#[cfg(windows)]
const APPROVED_RUN_SUBKEY: &str =
    r"Software\Microsoft\Windows\CurrentVersion\Explorer\StartupApproved\Run";
#[cfg(windows)]
const APPROVED_FOLDER_SUBKEY: &str =
    r"Software\Microsoft\Windows\CurrentVersion\Explorer\StartupApproved\StartupFolder";
#[cfg(windows)]
const VALUE_NAME: &str = "Storbyte";
#[cfg(windows)]
const SHORTCUT_NAME: &str = "Storbyte.lnk";

/// Command line written to the Run key (quoted exe + `--autostart`).
pub fn startup_command_line() -> Result<String> {
    let exe = std::env::current_exe().context("current_exe")?;
    Ok(format!("\"{}\" --autostart", path_display(&exe)))
}

fn path_display(path: &std::path::Path) -> String {
    path.to_string_lossy().replace('/', "\\")
}

#[cfg(windows)]
fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

#[cfg(windows)]
pub fn set_launch_at_startup(enabled: bool) -> Result<()> {
    use windows::core::PCWSTR;
    use windows::Win32::Foundation::ERROR_SUCCESS;
    use windows::Win32::System::Registry::{
        RegCloseKey, RegCreateKeyExW, RegDeleteValueW, RegSetValueExW, HKEY_CURRENT_USER,
        KEY_SET_VALUE, REG_CREATE_KEY_DISPOSITION, REG_OPTION_NON_VOLATILE, REG_SZ,
    };

    let name = wide(VALUE_NAME);
    let run_sub = wide(RUN_SUBKEY);

    let mut run_key = Default::default();
    let mut disposition = REG_CREATE_KEY_DISPOSITION::default();
    let status = unsafe {
        RegCreateKeyExW(
            HKEY_CURRENT_USER,
            PCWSTR(run_sub.as_ptr()),
            None,
            None,
            REG_OPTION_NON_VOLATILE,
            KEY_SET_VALUE,
            None,
            &mut run_key,
            Some(&mut disposition as *mut _),
        )
    };
    if status != ERROR_SUCCESS {
        return Err(anyhow!("RegCreateKeyExW(Run) failed: {status:?}"));
    }

    let run_result = (|| -> Result<()> {
        if enabled {
            let cmd = startup_command_line()?;
            let data: Vec<u16> = cmd.encode_utf16().chain(std::iter::once(0)).collect();
            let bytes = unsafe {
                std::slice::from_raw_parts(data.as_ptr() as *const u8, data.len() * 2)
            };
            let status = unsafe {
                RegSetValueExW(
                    run_key,
                    PCWSTR(name.as_ptr()),
                    None,
                    REG_SZ,
                    Some(bytes),
                )
            };
            if status != ERROR_SUCCESS {
                return Err(anyhow!("RegSetValueExW(Run) failed: {status:?}"));
            }
        } else {
            let status = unsafe { RegDeleteValueW(run_key, PCWSTR(name.as_ptr())) };
            if status != ERROR_SUCCESS && status.0 != 2 {
                return Err(anyhow!("RegDeleteValueW(Run) failed: {status:?}"));
            }
        }
        Ok(())
    })();
    unsafe {
        let _ = RegCloseKey(run_key);
    }
    run_result?;

    // StartupApproved entries — required for visibility/enable in Windows Startup apps UI.
    write_startup_approved(APPROVED_RUN_SUBKEY, VALUE_NAME, enabled)?;
    write_startup_approved(APPROVED_FOLDER_SUBKEY, SHORTCUT_NAME, enabled)?;

    // Also drop a Startup-folder shortcut so Task Manager / Settings reliably list it.
    sync_startup_folder_shortcut(enabled)?;

    Ok(())
}

#[cfg(windows)]
fn write_startup_approved(subkey: &str, value_name: &str, enabled: bool) -> Result<()> {
    use windows::core::PCWSTR;
    use windows::Win32::Foundation::ERROR_SUCCESS;
    use windows::Win32::System::Registry::{
        RegCloseKey, RegCreateKeyExW, RegDeleteValueW, RegSetValueExW, HKEY_CURRENT_USER,
        KEY_SET_VALUE, REG_BINARY, REG_CREATE_KEY_DISPOSITION, REG_OPTION_NON_VOLATILE,
    };

    let sub = wide(subkey);
    let name = wide(value_name);
    let mut key = Default::default();
    let mut disposition = REG_CREATE_KEY_DISPOSITION::default();
    let status = unsafe {
        RegCreateKeyExW(
            HKEY_CURRENT_USER,
            PCWSTR(sub.as_ptr()),
            None,
            None,
            REG_OPTION_NON_VOLATILE,
            KEY_SET_VALUE,
            None,
            &mut key,
            Some(&mut disposition as *mut _),
        )
    };
    if status != ERROR_SUCCESS {
        return Err(anyhow!("RegCreateKeyExW({subkey}) failed: {status:?}"));
    }

    let result = (|| -> Result<()> {
        if enabled {
            let enabled_bytes: [u8; 12] = [0x02, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0];
            let status = unsafe {
                RegSetValueExW(
                    key,
                    PCWSTR(name.as_ptr()),
                    None,
                    REG_BINARY,
                    Some(&enabled_bytes),
                )
            };
            if status != ERROR_SUCCESS {
                return Err(anyhow!("RegSetValueExW({subkey}) failed: {status:?}"));
            }
        } else {
            let status = unsafe { RegDeleteValueW(key, PCWSTR(name.as_ptr())) };
            if status != ERROR_SUCCESS && status.0 != 2 {
                return Err(anyhow!("RegDeleteValueW({subkey}) failed: {status:?}"));
            }
        }
        Ok(())
    })();
    unsafe {
        let _ = RegCloseKey(key);
    }
    result
}

#[cfg(windows)]
fn sync_startup_folder_shortcut(enabled: bool) -> Result<()> {
    use std::fs;
    use std::process::Command;

    let startup = std::env::var_os("APPDATA")
        .map(|a| {
            PathBuf::from(a)
                .join("Microsoft")
                .join("Windows")
                .join("Start Menu")
                .join("Programs")
                .join("Startup")
        })
        .ok_or_else(|| anyhow!("APPDATA missing"))?;

    let link = startup.join(SHORTCUT_NAME);
    if !enabled {
        let _ = fs::remove_file(&link);
        return Ok(());
    }

    let _ = fs::create_dir_all(&startup);
    let exe = std::env::current_exe().context("current_exe")?;
    let exe_s = path_display(&exe);
    let link_s = path_display(&link);
    let work = path_display(exe.parent().unwrap_or(std::path::Path::new("\\")));
    let ps = format!(
        "$ws = New-Object -ComObject WScript.Shell; \
         $s = $ws.CreateShortcut('{}'); \
         $s.TargetPath = '{}'; \
         $s.Arguments = '--autostart'; \
         $s.WorkingDirectory = '{}'; \
         $s.WindowStyle = 7; \
         $s.Description = 'Storbyte'; \
         $s.Save()",
        link_s.replace('\'', "''"),
        exe_s.replace('\'', "''"),
        work.replace('\'', "''"),
    );
    let out = Command::new("powershell")
        .args(["-NoProfile", "-NonInteractive", "-Command", &ps])
        .output()
        .context("powershell CreateShortcut")?;
    if !out.status.success() {
        let err = String::from_utf8_lossy(&out.stderr);
        return Err(anyhow!("shortcut create failed: {err}"));
    }
    Ok(())
}

#[cfg(not(windows))]
pub fn set_launch_at_startup(_enabled: bool) -> Result<()> {
    Ok(())
}

#[cfg(windows)]
pub fn is_launch_at_startup() -> bool {
    use windows::core::PCWSTR;
    use windows::Win32::Foundation::ERROR_SUCCESS;
    use windows::Win32::System::Registry::{
        RegCloseKey, RegOpenKeyExW, RegQueryValueExW, HKEY_CURRENT_USER, KEY_READ, REG_SZ,
    };

    let subkey = wide(RUN_SUBKEY);
    let name = wide(VALUE_NAME);

    let mut hkey = Default::default();
    let status = unsafe {
        RegOpenKeyExW(
            HKEY_CURRENT_USER,
            PCWSTR(subkey.as_ptr()),
            None,
            KEY_READ,
            &mut hkey,
        )
    };
    if status != ERROR_SUCCESS {
        return false;
    }

    let mut ty = REG_SZ;
    let mut size = 0u32;
    let status = unsafe {
        RegQueryValueExW(
            hkey,
            PCWSTR(name.as_ptr()),
            None,
            Some(&mut ty),
            None,
            Some(&mut size),
        )
    };
    unsafe {
        let _ = RegCloseKey(hkey);
    }
    status == ERROR_SUCCESS && size > 2
}

#[cfg(not(windows))]
pub fn is_launch_at_startup() -> bool {
    false
}

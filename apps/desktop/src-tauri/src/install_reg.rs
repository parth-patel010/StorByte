//! Register Storbyte under HKCU Uninstall so it appears in
//! Settings → Apps → Installed apps (without requiring an MSI/NSIS setup).

use anyhow::{anyhow, Context, Result};
use std::path::{Path, PathBuf};

#[cfg(windows)]
const UNINSTALL_SUBKEY: &str =
    r"Software\Microsoft\Windows\CurrentVersion\Uninstall\Storbyte";
#[cfg(windows)]
const DISPLAY_NAME: &str = "Storbyte";
#[cfg(windows)]
const PUBLISHER: &str = "Storbyte";
#[cfg(windows)]
const START_MENU_LINK: &str = "Storbyte.lnk";

/// Ensure Uninstall registry + Start Menu shortcut match the running exe.
pub fn register_installed_app() -> Result<()> {
    #[cfg(windows)]
    {
        let exe = std::env::current_exe().context("current_exe")?;
        write_uninstall_key(&exe)?;
        sync_start_menu_shortcut(true, &exe)?;
    }
    Ok(())
}

/// Remove Installed-apps registration, Start Menu link, and login autostart.
pub fn unregister_installed_app() -> Result<()> {
    #[cfg(windows)]
    {
        let _ = crate::autostart::set_launch_at_startup(false);
        delete_uninstall_key()?;
        if let Ok(exe) = std::env::current_exe() {
            let _ = sync_start_menu_shortcut(false, &exe);
        }
    }
    Ok(())
}

#[cfg(windows)]
fn path_display(path: &Path) -> String {
    path.to_string_lossy().replace('/', "\\")
}

#[cfg(windows)]
fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

#[cfg(windows)]
fn write_uninstall_key(exe: &Path) -> Result<()> {
    use windows::core::PCWSTR;
    use windows::Win32::Foundation::ERROR_SUCCESS;
    use windows::Win32::System::Registry::{
        RegCloseKey, RegCreateKeyExW, HKEY_CURRENT_USER, KEY_SET_VALUE,
        REG_CREATE_KEY_DISPOSITION, REG_OPTION_NON_VOLATILE,
    };

    let sub = wide(UNINSTALL_SUBKEY);
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
        return Err(anyhow!("RegCreateKeyExW(Uninstall) failed: {status:?}"));
    }

    let result = (|| -> Result<()> {
        let exe_s = path_display(exe);
        let install_dir = exe
            .parent()
            .map(path_display)
            .unwrap_or_else(|| exe_s.clone());
        let version = env!("CARGO_PKG_VERSION");
        let uninstall = format!("\"{exe_s}\" --uninstall");
        let icon = format!("{exe_s},0");
        let size_kb = std::fs::metadata(exe)
            .map(|m| (m.len() / 1024) as u32)
            .unwrap_or(0);
        let install_date = chrono::Local::now().format("%Y%m%d").to_string();

        set_sz(key, "DisplayName", DISPLAY_NAME)?;
        set_sz(key, "DisplayVersion", version)?;
        set_sz(key, "Publisher", PUBLISHER)?;
        set_sz(key, "InstallLocation", &install_dir)?;
        set_sz(key, "DisplayIcon", &icon)?;
        set_sz(key, "UninstallString", &uninstall)?;
        set_sz(key, "QuietUninstallString", &format!("\"{exe_s}\" --uninstall --quiet"))?;
        set_sz(key, "InstallDate", &install_date)?;
        set_dword(key, "NoModify", 1)?;
        set_dword(key, "NoRepair", 1)?;
        if size_kb > 0 {
            set_dword(key, "EstimatedSize", size_kb)?;
        }
        Ok(())
    })();

    unsafe {
        let _ = RegCloseKey(key);
    }
    result
}

#[cfg(windows)]
fn set_sz(
    key: windows::Win32::System::Registry::HKEY,
    name: &str,
    value: &str,
) -> Result<()> {
    use windows::core::PCWSTR;
    use windows::Win32::Foundation::ERROR_SUCCESS;
    use windows::Win32::System::Registry::{RegSetValueExW, REG_SZ};

    let n = wide(name);
    let data: Vec<u16> = value.encode_utf16().chain(std::iter::once(0)).collect();
    let bytes = unsafe { std::slice::from_raw_parts(data.as_ptr() as *const u8, data.len() * 2) };
    let status = unsafe { RegSetValueExW(key, PCWSTR(n.as_ptr()), None, REG_SZ, Some(bytes)) };
    if status != ERROR_SUCCESS {
        return Err(anyhow!("RegSetValueExW({name}) failed: {status:?}"));
    }
    Ok(())
}

#[cfg(windows)]
fn set_dword(
    key: windows::Win32::System::Registry::HKEY,
    name: &str,
    value: u32,
) -> Result<()> {
    use windows::core::PCWSTR;
    use windows::Win32::Foundation::ERROR_SUCCESS;
    use windows::Win32::System::Registry::{RegSetValueExW, REG_DWORD};

    let n = wide(name);
    let bytes = value.to_le_bytes();
    let status = unsafe { RegSetValueExW(key, PCWSTR(n.as_ptr()), None, REG_DWORD, Some(&bytes)) };
    if status != ERROR_SUCCESS {
        return Err(anyhow!("RegSetValueExW({name}) failed: {status:?}"));
    }
    Ok(())
}

#[cfg(windows)]
fn delete_uninstall_key() -> Result<()> {
    use windows::core::PCWSTR;
    use windows::Win32::Foundation::ERROR_SUCCESS;
    use windows::Win32::System::Registry::{
        RegDeleteTreeW, RegOpenKeyExW, RegCloseKey, HKEY_CURRENT_USER, KEY_WRITE,
    };

    let parent = wide(r"Software\Microsoft\Windows\CurrentVersion\Uninstall");
    let mut hkey = Default::default();
    let status = unsafe {
        RegOpenKeyExW(
            HKEY_CURRENT_USER,
            PCWSTR(parent.as_ptr()),
            None,
            KEY_WRITE,
            &mut hkey,
        )
    };
    if status != ERROR_SUCCESS {
        return Ok(());
    }
    let child = wide("Storbyte");
    let _ = unsafe { RegDeleteTreeW(hkey, PCWSTR(child.as_ptr())) };
    unsafe {
        let _ = RegCloseKey(hkey);
    }
    Ok(())
}

#[cfg(windows)]
fn start_menu_programs_dir() -> Result<PathBuf> {
    let appdata = std::env::var_os("APPDATA").ok_or_else(|| anyhow!("APPDATA missing"))?;
    Ok(PathBuf::from(appdata)
        .join("Microsoft")
        .join("Windows")
        .join("Start Menu")
        .join("Programs"))
}

#[cfg(windows)]
fn sync_start_menu_shortcut(enabled: bool, exe: &Path) -> Result<()> {
    use std::fs;
    use std::process::Command;

    let programs = start_menu_programs_dir()?;
    let link = programs.join(START_MENU_LINK);
    if !enabled {
        let _ = fs::remove_file(&link);
        return Ok(());
    }

    let _ = fs::create_dir_all(&programs);
    let exe_s = path_display(exe);
    let link_s = path_display(&link);
    let work = path_display(exe.parent().unwrap_or(Path::new("\\")));
    let ps = format!(
        "$ws = New-Object -ComObject WScript.Shell; \
         $s = $ws.CreateShortcut('{}'); \
         $s.TargetPath = '{}'; \
         $s.WorkingDirectory = '{}'; \
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
        return Err(anyhow!("Start Menu shortcut failed: {err}"));
    }
    Ok(())
}

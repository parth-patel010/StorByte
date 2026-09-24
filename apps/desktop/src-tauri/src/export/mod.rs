use anyhow::{anyhow, Context, Result};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

use crate::config::load_settings;
use crate::metadata::{normalize_virtual_path, MetadataDb};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StorbyteLink {
    pub v: u32,
    pub app: String,
    pub account_id: String,
    pub remote_id: String,
    pub name: String,
    pub size: u64,
    pub mime: Option<String>,
    pub virtual_path: String,
}

impl StorbyteLink {
    pub fn to_pretty_json(&self) -> Result<String> {
        Ok(serde_json::to_string_pretty(self)?)
    }
}

pub fn link_from_node(
    account_id: &str,
    remote_id: &str,
    name: &str,
    size: u64,
    mime: Option<String>,
    virtual_path: &str,
) -> StorbyteLink {
    StorbyteLink {
        v: 1,
        app: "Storbyte".into(),
        account_id: account_id.into(),
        remote_id: remote_id.into(),
        name: name.into(),
        size,
        mime,
        virtual_path: normalize_virtual_path(virtual_path),
    }
}

pub fn write_storbyte_link(dest_dir: &Path, link: &StorbyteLink) -> Result<PathBuf> {
    fs::create_dir_all(dest_dir)?;
    let file_name = format!("{}.storbytelink", sanitize_filename(&link.name));
    let path = dest_dir.join(file_name);
    fs::write(&path, link.to_pretty_json()?)?;
    Ok(path)
}

pub fn read_storbyte_link(path: &Path) -> Result<StorbyteLink> {
    let raw = fs::read_to_string(path).with_context(|| format!("read {}", path.display()))?;
    Ok(serde_json::from_str(&raw)?)
}

pub fn create_link_for_virtual_path(db: &MetadataDb, virtual_path: &str, dest_dir: &Path) -> Result<PathBuf> {
    let node = db
        .get_node_by_path(virtual_path)?
        .ok_or_else(|| anyhow!("path not found in index: {virtual_path}"))?;
    if node.is_directory {
        return Err(anyhow!("cannot create link for a directory in prototype"));
    }
    let link = link_from_node(
        &node.account_id,
        &node.remote_id,
        &node.name,
        node.size as u64,
        node.mime_type.clone(),
        &node.virtual_path,
    );
    write_storbyte_link(dest_dir, &link)
}

/// Resolve a .storbytelink to a mounted drive path and open it.
pub fn open_link(db: &MetadataDb, path: &Path) -> Result<()> {
    let link = read_storbyte_link(path)?;
    let settings = load_settings();
    let drive = settings.drive_letter.chars().next().unwrap_or('V');
    // Prefer indexed virtual path; fall back to lookup by remote id.
    let vpath = if db.get_node_by_path(&link.virtual_path)?.is_some() {
        link.virtual_path.clone()
    } else {
        link.virtual_path.clone()
    };
    let windows_path = format!("{drive}:{}", vpath.replace('/', "\\"));
    open::that(&windows_path).with_context(|| format!("open {windows_path}"))?;
    Ok(())
}

fn sanitize_filename(name: &str) -> String {
    name.chars()
        .map(|c| match c {
            '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*' => '_',
            _ => c,
        })
        .collect()
}

/// Register HKCU file association for .storbytelink and a shell verb for "Copy as Storbyte link".
pub fn register_shell_integration(exe: &Path) -> Result<()> {
    #[cfg(windows)]
    {
        use std::process::Command;
        let exe_str = exe.display().to_string();
        let pairs = [
            (r"HKCU\Software\Classes\.storbytelink", "", "Storbyte.Link"),
            (r"HKCU\Software\Classes\Storbyte.Link", "", "Storbyte Zero-bit Link"),
            (
                r"HKCU\Software\Classes\Storbyte.Link\shell\open\command",
                "",
                &format!("\"{exe_str}\" --open-link \"%1\""),
            ),
            (
                r"HKCU\Software\Classes\*\shell\StorbyteCopyLink",
                "",
                "Copy as Storbyte link...",
            ),
            (
                r"HKCU\Software\Classes\*\shell\StorbyteCopyLink\command",
                "",
                &format!("\"{exe_str}\" --export-link \"%1\""),
            ),
        ];
        for (key, name, data) in pairs {
            let mut cmd = Command::new("reg");
            cmd.args(["add", key, "/ve", "/d", data, "/f"]);
            if !name.is_empty() {
                cmd = Command::new("reg");
                cmd.args(["add", key, "/v", name, "/d", data, "/f"]);
            }
            let status = cmd.status()?;
            if !status.success() {
                tracing::warn!(%key, "reg add failed");
            }
        }
    }
    #[cfg(not(windows))]
    {
        let _ = exe;
    }
    Ok(())
}

pub fn export_link_cli(source_path: &str) -> Result<()> {
    let settings = load_settings();
    if !settings.zero_bit_export {
        return Err(anyhow!(
            "Zero-bit export is OFF. Enable it in Settings, or copy the file normally."
        ));
    }
    let db = MetadataDb::open_default()?;
    let source = PathBuf::from(source_path);
    let drive_prefix = format!("{}:", settings.drive_letter);
    let source_str = source.to_string_lossy();
    let virtual_path = if source_str.starts_with(&drive_prefix)
        || source_str.starts_with(&drive_prefix.to_ascii_lowercase())
    {
        let rest = &source_str[drive_prefix.len()..];
        normalize_virtual_path(rest)
    } else {
        return Err(anyhow!(
            "Source is not on the Storbyte drive ({drive_prefix}). Open the file from the virtual drive first."
        ));
    };
    let dest = directories::UserDirs::new()
        .and_then(|u| u.desktop_dir().map(|p| p.to_path_buf()))
        .unwrap_or_else(|| PathBuf::from("."));
    let out = create_link_for_virtual_path(&db, &virtual_path, &dest)?;
    tracing::info!(path = %out.display(), "wrote Storbyte link");
    Ok(())
}

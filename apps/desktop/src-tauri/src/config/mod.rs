use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum UploadPlacement {
    #[default]
    MostFree,
    LeastFree,
    Ask,
}

fn default_true() -> bool {
    true
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppSettings {
    pub drive_letter: String,
    pub zero_bit_export: bool,
    pub cache_bytes: u64,
    pub default_account_id: Option<String>,
    pub onboarded: bool,
    #[serde(default)]
    pub intro_seen: bool,
    #[serde(default)]
    pub upload_placement: UploadPlacement,
    /// Start Storbyte when Windows signs in (tray + auto-mount).
    #[serde(default = "default_true")]
    pub launch_at_startup: bool,
    /// When the app is running, wait for network then mount automatically.
    #[serde(default = "default_true")]
    pub auto_mount_on_launch: bool,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            drive_letter: "V".into(),
            zero_bit_export: true,
            cache_bytes: 512 * 1024 * 1024,
            default_account_id: None,
            onboarded: false,
            intro_seen: false,
            upload_placement: UploadPlacement::MostFree,
            launch_at_startup: true,
            auto_mount_on_launch: true,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SettingsPatch {
    pub drive_letter: Option<String>,
    pub zero_bit_export: Option<bool>,
    pub cache_bytes: Option<u64>,
    pub default_account_id: Option<Option<String>>,
    pub onboarded: Option<bool>,
    pub intro_seen: Option<bool>,
    pub upload_placement: Option<UploadPlacement>,
    pub launch_at_startup: Option<bool>,
    pub auto_mount_on_launch: Option<bool>,
}

pub fn data_dir() -> PathBuf {
    let base = directories::ProjectDirs::from("com", "Storbyte", "Storbyte")
        .map(|d| d.data_dir().to_path_buf())
        .unwrap_or_else(|| PathBuf::from(".Storbyte"));
    fs::create_dir_all(&base).ok();
    base
}

pub fn settings_path() -> PathBuf {
    data_dir().join("settings.json")
}

pub fn load_settings() -> AppSettings {
    let path = settings_path();
    if let Ok(raw) = fs::read_to_string(&path) {
        if let Ok(s) = serde_json::from_str::<AppSettings>(&raw) {
            return s;
        }
    }
    let defaults = AppSettings::default();
    let _ = save_settings(&defaults);
    defaults
}

pub fn save_settings(settings: &AppSettings) -> anyhow::Result<()> {
    let path = settings_path();
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, serde_json::to_string_pretty(settings)?)?;
    Ok(())
}

pub fn apply_patch(mut current: AppSettings, patch: SettingsPatch) -> AppSettings {
    if let Some(v) = patch.drive_letter {
        current.drive_letter = v.chars().next().unwrap_or('V').to_ascii_uppercase().to_string();
    }
    if let Some(v) = patch.zero_bit_export {
        current.zero_bit_export = v;
    }
    if let Some(v) = patch.cache_bytes {
        current.cache_bytes = v;
    }
    if let Some(v) = patch.default_account_id {
        current.default_account_id = v;
    }
    if let Some(v) = patch.onboarded {
        current.onboarded = v;
    }
    if let Some(v) = patch.intro_seen {
        current.intro_seen = v;
    }
    if let Some(v) = patch.upload_placement {
        current.upload_placement = v;
    }
    if let Some(v) = patch.launch_at_startup {
        current.launch_at_startup = v;
    }
    if let Some(v) = patch.auto_mount_on_launch {
        current.auto_mount_on_launch = v;
    }
    current
}

pub fn google_client_id() -> String {
    runtime_or_embedded_obf("GOOGLE_CLIENT_ID", option_env!("GOOGLE_CLIENT_ID_OBF"))
}

pub fn google_client_secret() -> String {
    runtime_or_embedded_obf("GOOGLE_CLIENT_SECRET", option_env!("GOOGLE_CLIENT_SECRET_OBF"))
}

/// Prefer process env / optional `.env`, then XOR-decoded compile-time embed.
fn runtime_or_embedded_obf(env_key: &str, embedded_hex: Option<&str>) -> String {
    if let Ok(v) = std::env::var(env_key) {
        if !v.is_empty() {
            return v;
        }
    }
    embedded_hex
        .filter(|s| !s.is_empty())
        .and_then(decode_obf_hex)
        .unwrap_or_default()
}

fn decode_obf_hex(hex: &str) -> Option<String> {
    let key: u8 = option_env!("STORBYTE_OBF_KEY")?.parse().ok()?;
    if hex.len() % 2 != 0 {
        return None;
    }
    let mut out = Vec::with_capacity(hex.len() / 2);
    for (i, chunk) in hex.as_bytes().chunks(2).enumerate() {
        let h = std::str::from_utf8(chunk).ok()?;
        let b = u8::from_str_radix(h, 16).ok()?;
        out.push(b ^ key.wrapping_add(i as u8).wrapping_mul(17));
    }
    String::from_utf8(out).ok()
}

pub fn oauth_redirect() -> String {
    std::env::var("Storbyte_OAUTH_REDIRECT")
        .unwrap_or_else(|_| "http://127.0.0.1:17832/oauth/callback".into())
}

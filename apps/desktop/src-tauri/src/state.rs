use crate::auth::{self, TokenSet};
use crate::cache::BlockCache;
use crate::prefetch::PrefetchEngine;
use crate::config::{self, AppSettings, SettingsPatch};
use crate::export;
use crate::filesystem::{DriveStatus, FsShared, MountManager};
use crate::metadata::{self, AccountRow, MetadataDb};
use crate::providers::GoogleDriveProvider;
use crate::providers::StorageProvider;
use crate::scheduler::RequestScheduler;
use anyhow::{anyhow, Result};
use parking_lot::RwLock;
use serde::Serialize;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tauri::{AppHandle, Emitter, Manager};
use tokio::runtime::Handle;

#[derive(Clone, Serialize)]
pub struct FilePreviewDto {
    pub name: String,
    pub mime_type: Option<String>,
    pub size: u64,
    pub account_id: String,
    pub virtual_path: String,
    pub data_base64: String,
    pub truncated: bool,
    pub on_disk_bytes: u64,
}

#[derive(Clone, Serialize)]
pub struct AccountInfo {
    pub id: String,
    pub email: String,
    pub provider: String,
    pub status: String,
    pub priority: i32,
    pub quota_bytes: Option<i64>,
    pub usage_bytes: Option<i64>,
    pub last_indexed_at: Option<String>,
}

#[derive(Clone, Serialize)]
pub struct AppStateDto {
    pub settings: AppSettings,
    pub accounts: Vec<AccountInfo>,
    pub drive: DriveStatus,
}

pub struct AppState {
    pub db: Arc<MetadataDb>,
    pub settings: Arc<RwLock<AppSettings>>,
    pub cache: Arc<BlockCache>,
    pub prefetch: Arc<PrefetchEngine>,
    pub scheduler: Arc<RequestScheduler>,
    pub providers: Arc<RwLock<Vec<Arc<dyn StorageProvider>>>>,
    pub mount: Arc<MountManager>,
    pub runtime: Handle,
    change_watch_started: AtomicBool,
    /// Shared with the mounted FsShared for ask-every-time upload routing.
    pub upload_ask: Arc<crate::filesystem::UploadAskGate>,
    pub app_for_fs: Arc<parking_lot::RwLock<Option<AppHandle>>>,
    /// Set while Google OAuth is waiting for the browser callback.
    pub oauth_cancel: Arc<AtomicBool>,
}

impl AppState {
    pub fn new(runtime: Handle) -> Result<Self> {
        let settings = Arc::new(RwLock::new(config::load_settings()));
        let db = Arc::new(MetadataDb::open_default()?);
        let cache = Arc::new(BlockCache::new(settings.read().cache_bytes));
        let prefetch = Arc::new(PrefetchEngine::new());
        let scheduler = Arc::new(RequestScheduler::new(4));
        let providers = Arc::new(RwLock::new(Vec::new()));
        let mount = Arc::new(MountManager::new(settings.read().drive_letter.clone()));

        let state = Self {
            db: db.clone(),
            settings: settings.clone(),
            cache: cache.clone(),
            prefetch: prefetch.clone(),
            scheduler: scheduler.clone(),
            providers: providers.clone(),
            mount: mount.clone(),
            runtime,
            change_watch_started: AtomicBool::new(false),
            upload_ask: Arc::new(crate::filesystem::UploadAskGate::new()),
            app_for_fs: Arc::new(parking_lot::RwLock::new(None)),
            oauth_cancel: Arc::new(AtomicBool::new(false)),
        };
        state.reload_providers()?;
        Ok(state)
    }

    pub fn fs_shared(&self) -> Arc<FsShared> {
        Arc::new(FsShared {
            db: self.db.clone(),
            settings: self.settings.clone(),
            cache: self.cache.clone(),
            prefetch: self.prefetch.clone(),
            scheduler: self.scheduler.clone(),
            providers: self.providers.clone(),
            runtime: self.runtime.clone(),
            app: self.app_for_fs.clone(),
            upload_ask: self.upload_ask.clone(),
        })
    }

    pub fn resolve_upload_account(&self, account_id: String) -> bool {
        self.upload_ask.resolve(Some(account_id))
    }

    pub fn cancel_upload_account_pick(&self) -> bool {
        self.upload_ask.resolve(None)
    }

    pub fn reload_providers(&self) -> Result<()> {
        let accounts = self.db.list_accounts()?;
        let mut list: Vec<Arc<dyn StorageProvider>> = Vec::new();
        for a in accounts {
            if a.provider == "google" && a.status != "disconnected" {
                list.push(Arc::new(GoogleDriveProvider::new(
                    a.id.clone(),
                    self.scheduler.clone(),
                )));
            }
        }
        *self.providers.write() = list;
        Ok(())
    }

    pub fn dto(&self) -> AppStateDto {
        let accounts = self
            .db
            .list_accounts()
            .unwrap_or_default()
            .into_iter()
            .map(|a| AccountInfo {
                id: a.id,
                email: a.email,
                provider: a.provider,
                status: a.status,
                priority: a.priority,
                quota_bytes: a.quota_bytes,
                usage_bytes: a.usage_bytes,
                last_indexed_at: a.last_indexed_at,
            })
            .collect();
        AppStateDto {
            settings: self.settings.read().clone(),
            accounts,
            drive: self.mount.status(),
        }
    }

    pub fn update_settings(&self, patch: SettingsPatch) -> Result<AppSettings> {
        let prev_launch = self.settings.read().launch_at_startup;
        let next = config::apply_patch(self.settings.read().clone(), patch);
        config::save_settings(&next)?;
        self.cache.set_max_bytes(next.cache_bytes);
        self.mount.set_drive_letter(next.drive_letter.clone());
        if next.launch_at_startup != prev_launch {
            if let Err(e) = crate::autostart::set_launch_at_startup(next.launch_at_startup) {
                tracing::warn!(error = %e, "launch-at-startup update failed");
            }
        }
        *self.settings.write() = next.clone();
        Ok(next)
    }

    pub async fn refresh_all_quotas(&self) -> Result<Vec<AccountInfo>> {
        self.reload_providers()?;
        let providers = self.providers.read().clone();
        for p in providers {
            if let Ok(Some(q)) = p.about_quota().await {
                let mut accounts = self.db.list_accounts()?;
                if let Some(a) = accounts.iter_mut().find(|a| a.id == p.account_id()) {
                    a.quota_bytes = q.limit_bytes.map(|v| v as i64);
                    a.usage_bytes = q.usage_bytes.map(|v| v as i64);
                    a.status = "connected".into();
                    self.db.upsert_account(a)?;
                }
            }
        }
        Ok(self.dto().accounts)
    }

    pub fn cancel_google_oauth(&self) {
        self.oauth_cancel.store(true, Ordering::SeqCst);
    }

    pub async fn connect_google(&self) -> Result<AccountInfo> {
        let (account_id, tokens) = auth::connect_google_oauth(self.oauth_cancel.clone()).await?;
        let email = tokens
            .email
            .clone()
            .unwrap_or_else(|| format!("{account_id}@unknown"));
        let priority = (self.db.list_accounts()?.len() as i32) + 1;
        let mut row = AccountRow {
            id: account_id.clone(),
            email: email.clone(),
            provider: "google".into(),
            status: "connected".into(),
            priority,
            quota_bytes: None,
            usage_bytes: None,
            last_indexed_at: None,
            change_page_token: None,
            drive_root_id: None,
        };
        self.db.upsert_account(&row)?;
        if self.settings.read().default_account_id.is_none() {
            let _ = self.update_settings(SettingsPatch {
                default_account_id: Some(Some(account_id.clone())),
                onboarded: Some(true),
                ..Default::default()
            });
        }
        self.reload_providers()?;
        // Best-effort quota fetch (clone Arc before await so we don't hold the lock)
        let provider = self
            .providers
            .read()
            .iter()
            .find(|p| p.account_id() == account_id)
            .cloned();
        if let Some(p) = provider {
            if let Ok(Some(q)) = p.about_quota().await {
                row.quota_bytes = q.limit_bytes.map(|v| v as i64);
                row.usage_bytes = q.usage_bytes.map(|v| v as i64);
                self.db.upsert_account(&row)?;
            }
        }
        Ok(AccountInfo {
            id: account_id,
            email,
            provider: "google".into(),
            status: "connected".into(),
            priority,
            quota_bytes: row.quota_bytes,
            usage_bytes: row.usage_bytes,
            last_indexed_at: None,
        })
    }

    pub fn disconnect(&self, account_id: &str) -> Result<()> {
        // Soft disconnect: keep metadata, drop live tokens/provider until reconnect.
        let _ = auth::delete_tokens(account_id);
        self.db.set_account_status(account_id, "disconnected")?;
        self.reload_providers()?;
        Ok(())
    }

    /// Full sign-out: wipe tokens, indexed files for the account, and the account row.
    /// Unmounts the drive when no connected providers remain.
    pub fn remove_account(&self, account_id: &str) -> Result<()> {
        let _ = auth::delete_tokens(account_id);
        let _ = self.db.clear_account_nodes(account_id);
        self.db.delete_account(account_id)?;
        if self.settings.read().default_account_id.as_deref() == Some(account_id) {
            let _ = self.update_settings(SettingsPatch {
                default_account_id: Some(None),
                ..Default::default()
            });
        }
        self.reload_providers()?;
        if self.providers.read().is_empty() {
            let _ = self.unmount_drive();
            // Drop any leftover catalog so the UI cannot show stale folders.
            let _ = self.db.clear_all_nodes_except_root();
            self.cache.clear();
        } else {
            // Keep drive mounted; force providers/volume to reflect remaining accounts.
            let _ = self.reload_providers();
        }
        Ok(())
    }

    /// Sign out = full remove (no leftover Connected-accounts rows or Drive tree).
    pub fn sign_out(&self, account_id: &str) -> Result<()> {
        self.remove_account(account_id)
    }

    pub async fn reconnect_account(&self, account_id: &str) -> Result<AccountInfo> {
        let existing = self
            .db
            .get_account(account_id)?
            .ok_or_else(|| anyhow!("account not found"))?;
        let (new_id, tokens) = auth::connect_google_oauth(self.oauth_cancel.clone()).await?;
        // Prefer keeping the same row if Google returns the same subject id.
        let id = if new_id == account_id {
            account_id.to_string()
        } else if tokens.email.as_deref() == Some(existing.email.as_str()) {
            // Same email, different id storage — migrate tokens onto existing id.
            if let Ok(Some(t)) = auth::load_tokens(&new_id) {
                let _ = auth::store_tokens(account_id, &t);
                let _ = auth::delete_tokens(&new_id);
            }
            account_id.to_string()
        } else {
            new_id
        };
        let email = tokens.email.unwrap_or(existing.email);
        let row = AccountRow {
            id: id.clone(),
            email: email.clone(),
            provider: "google".into(),
            status: "connected".into(),
            priority: existing.priority,
            quota_bytes: existing.quota_bytes,
            usage_bytes: existing.usage_bytes,
            last_indexed_at: existing.last_indexed_at,
            change_page_token: existing.change_page_token,
            drive_root_id: existing.drive_root_id,
        };
        self.db.upsert_account(&row)?;
        self.reload_providers()?;
        // Refresh quota after reconnect.
        let mut out = AccountInfo {
            id: id.clone(),
            email,
            provider: "google".into(),
            status: "connected".into(),
            priority: row.priority,
            quota_bytes: row.quota_bytes,
            usage_bytes: row.usage_bytes,
            last_indexed_at: row.last_indexed_at.clone(),
        };
        let provider = self
            .providers
            .read()
            .iter()
            .find(|p| p.account_id() == id)
            .cloned();
        if let Some(p) = provider {
            if let Ok(Some(q)) = p.about_quota().await {
                let mut updated = row;
                updated.quota_bytes = q.limit_bytes.map(|v| v as i64);
                updated.usage_bytes = q.usage_bytes.map(|v| v as i64);
                self.db.upsert_account(&updated)?;
                out.quota_bytes = updated.quota_bytes;
                out.usage_bytes = updated.usage_bytes;
            }
        }
        Ok(out)
    }

    pub async fn sync_all<F>(&self, mut on_progress: F) -> Result<()>
    where
        F: FnMut(metadata::SyncProgress) + Send,
    {
        self.sync_all_inner(false, &mut on_progress).await
    }

    pub async fn sync_all_shallow<F>(&self, mut on_progress: F) -> Result<()>
    where
        F: FnMut(metadata::SyncProgress) + Send,
    {
        self.sync_all_inner(true, &mut on_progress).await
    }

    async fn sync_all_inner<F>(&self, shallow: bool, on_progress: &mut F) -> Result<()>
    where
        F: FnMut(metadata::SyncProgress) + Send,
    {
        self.reload_providers()?;
        let providers = self.providers.read().clone();
        if providers.is_empty() {
            return Err(anyhow!(
                "No connected Google account to sync. Connect an account first."
            ));
        }

        self.mount.set_indexing(true);
        self.mount.set_error(None);

        let account_total = providers.len() as u32;
        let accounts = self.db.list_accounts().unwrap_or_default();
        let labels: Vec<String> = providers
            .iter()
            .map(|p| {
                accounts
                    .iter()
                    .find(|a| a.id == p.account_id())
                    .map(|a| a.email.clone())
                    .unwrap_or_else(|| p.account_id().to_string())
            })
            .collect();

        let account_statuses: Vec<metadata::AccountSyncStatus> = labels
            .iter()
            .map(|label| metadata::AccountSyncStatus {
                label: label.clone(),
                status: "waiting".into(),
                items_indexed: 0,
            })
            .collect();

        let sync_msg: &str = if shallow {
            "Loading your Drive folders…"
        } else {
            "Fetching your data from drive"
        };

        let start = metadata::SyncProgress {
            active: true,
            percent: 0.0,
            message: sync_msg.into(),
            account_total,
            accounts: account_statuses.clone(),
            ..Default::default()
        };
        self.mount.set_sync_progress(Some(start.clone()));
        on_progress(start);

        // Clean slate once; accounts then insert in parallel (metadata only).
        let _ = self.db.clear_all_nodes_except_root();

        let progress_lock =
            std::sync::Arc::new(std::sync::Mutex::new(account_statuses));
        let items_lock = std::sync::Arc::new(std::sync::atomic::AtomicU64::new(0));
        let done_lock = std::sync::Arc::new(std::sync::atomic::AtomicU32::new(0));
        // Shared high-water mark so parallel accounts never report a lower %.
        let max_percent = std::sync::Arc::new(std::sync::atomic::AtomicU64::new(0));
        let (progress_tx, mut progress_rx) =
            tokio::sync::mpsc::unbounded_channel::<metadata::SyncProgress>();

        // Parallel accounts: up to 3 concurrent (within 2–4 band when multiple).
        let concurrency = providers.len().clamp(1, 3);
        let semaphore = std::sync::Arc::new(tokio::sync::Semaphore::new(concurrency));
        let db = self.db.clone();
        let mount = self.mount.clone();

        let mut set = tokio::task::JoinSet::new();
        for (i, p) in providers.into_iter().enumerate() {
            let semaphore = semaphore.clone();
            let db = db.clone();
            let mount = mount.clone();
            let label = labels[i].clone();
            let progress_lock = progress_lock.clone();
            let items_lock = items_lock.clone();
            let done_lock = done_lock.clone();
            let max_percent = max_percent.clone();
            let progress_tx = progress_tx.clone();
            let account_total = account_total;
            let shallow = shallow;
            let sync_msg = sync_msg.to_string();

            set.spawn(async move {
                let _permit = match semaphore.acquire_owned().await {
                    Ok(p) => p,
                    Err(_) => {
                        return Err(anyhow!("sync semaphore closed"));
                    }
                };

                let bump_percent = |raw: f64| -> f64 {
                    let raw_bits = (raw.clamp(0.0, 99.5) * 1000.0).round() as u64;
                    let mut cur = max_percent.load(std::sync::atomic::Ordering::Relaxed);
                    loop {
                        if raw_bits <= cur {
                            return cur as f64 / 1000.0;
                        }
                        match max_percent.compare_exchange_weak(
                            cur,
                            raw_bits,
                            std::sync::atomic::Ordering::Relaxed,
                            std::sync::atomic::Ordering::Relaxed,
                        ) {
                            Ok(_) => return raw_bits as f64 / 1000.0,
                            Err(v) => cur = v,
                        }
                    }
                };

                {
                    let mut st = progress_lock.lock().unwrap();
                    if let Some(s) = st.get_mut(i) {
                        s.status = "indexing".into();
                    }
                    let snap = metadata::SyncProgress {
                        active: true,
                        percent: bump_percent(
                            ((i as f64 + 0.08) / account_total.max(1) as f64) * 100.0,
                        ),
                        message: sync_msg.clone(),
                        account_index: i as u32,
                        account_total,
                        account_label: label.clone(),
                        accounts: st.clone(),
                        ..Default::default()
                    };
                    mount.set_sync_progress(Some(snap.clone()));
                    let _ = progress_tx.send(snap);
                }

                let progress_lock2 = progress_lock.clone();
                let items_lock2 = items_lock.clone();
                let done_lock2 = done_lock.clone();
                let progress_tx2 = progress_tx.clone();
                let mount2 = mount.clone();
                let label2 = label.clone();
                let max_percent2 = max_percent.clone();
                let sync_msg2 = sync_msg.clone();

                let result = if shallow {
                    metadata::sync_account_root_only(
                        db.as_ref(),
                        p.as_ref(),
                        i as u32,
                        account_total,
                        &label,
                        move |mut prog| {
                            let mut st = progress_lock2.lock().unwrap();
                            if let Some(s) = st.get_mut(i) {
                                s.status = "indexing".into();
                            }
                            prog.accounts = st.clone();
                            drop(st);
                            prog.message = sync_msg2.clone();
                            let completed = done_lock2.load(std::sync::atomic::Ordering::Relaxed);
                            let prior_items = items_lock2.load(std::sync::atomic::Ordering::Relaxed);
                            let base = completed as f64 / account_total.max(1) as f64;
                            let within = (prog.percent / 100.0) / account_total.max(1) as f64;
                            let raw = ((base + within) * 100.0).clamp(0.0, 99.5);
                            let raw_bits = (raw * 1000.0).round() as u64;
                            let mut cur = max_percent2.load(std::sync::atomic::Ordering::Relaxed);
                            let mono = loop {
                                if raw_bits <= cur {
                                    break cur as f64 / 1000.0;
                                }
                                match max_percent2.compare_exchange_weak(
                                    cur,
                                    raw_bits,
                                    std::sync::atomic::Ordering::Relaxed,
                                    std::sync::atomic::Ordering::Relaxed,
                                ) {
                                    Ok(_) => break raw_bits as f64 / 1000.0,
                                    Err(v) => cur = v,
                                }
                            };
                            prog.percent = mono;
                            prog.items_indexed = prior_items.saturating_add(prog.items_indexed);
                            prog.account_label = label2.clone();
                            mount2.set_sync_progress(Some(prog.clone()));
                            let _ = progress_tx2.send(prog);
                        },
                    )
                    .await
                } else {
                    metadata::sync_account(
                        db.as_ref(),
                        p.as_ref(),
                        i as u32,
                        account_total,
                        &label,
                        move |mut prog| {
                            let mut st = progress_lock2.lock().unwrap();
                            if let Some(s) = st.get_mut(i) {
                                s.status = "indexing".into();
                            }
                            prog.accounts = st.clone();
                            drop(st);
                            prog.message = sync_msg2.clone();
                            let completed = done_lock2.load(std::sync::atomic::Ordering::Relaxed);
                            let prior_items = items_lock2.load(std::sync::atomic::Ordering::Relaxed);
                            let base = completed as f64 / account_total.max(1) as f64;
                            let within = (prog.percent / 100.0) / account_total.max(1) as f64;
                            let raw = ((base + within) * 100.0).clamp(0.0, 99.5);
                            let raw_bits = (raw * 1000.0).round() as u64;
                            let mut cur = max_percent2.load(std::sync::atomic::Ordering::Relaxed);
                            let mono = loop {
                                if raw_bits <= cur {
                                    break cur as f64 / 1000.0;
                                }
                                match max_percent2.compare_exchange_weak(
                                    cur,
                                    raw_bits,
                                    std::sync::atomic::Ordering::Relaxed,
                                    std::sync::atomic::Ordering::Relaxed,
                                ) {
                                    Ok(_) => break raw_bits as f64 / 1000.0,
                                    Err(v) => cur = v,
                                }
                            };
                            prog.percent = mono;
                            prog.items_indexed = prior_items.saturating_add(prog.items_indexed);
                            prog.account_label = label2.clone();
                            mount2.set_sync_progress(Some(prog.clone()));
                            let _ = progress_tx2.send(prog);
                        },
                    )
                    .await
                };

                let n = match &result {
                    Ok(n) => *n,
                    Err(_) => 0,
                };
                items_lock.fetch_add(n, std::sync::atomic::Ordering::Relaxed);
                done_lock.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                {
                    let mut st = progress_lock.lock().unwrap();
                    if let Some(s) = st.get_mut(i) {
                        s.status = if result.is_ok() {
                            "complete".into()
                        } else {
                            "error".into()
                        };
                        s.items_indexed = n;
                    }
                    let completed = done_lock.load(std::sync::atomic::Ordering::Relaxed);
                    let items = items_lock.load(std::sync::atomic::Ordering::Relaxed);
                    let snap = metadata::SyncProgress {
                        active: true,
                        percent: bump_percent(
                            completed as f64 / account_total.max(1) as f64 * 100.0,
                        ),
                        items_indexed: items,
                        message: sync_msg.clone(),
                        account_index: i as u32,
                        account_total,
                        account_label: label,
                        accounts: st.clone(),
                        ..Default::default()
                    };
                    mount.set_sync_progress(Some(snap.clone()));
                    let _ = progress_tx.send(snap);
                }
                result.map(|_| ())
            });
        }
        drop(progress_tx);

        let mut result = Ok(());
        while !set.is_empty() {
            tokio::select! {
                Some(prog) = progress_rx.recv() => {
                    on_progress(prog);
                }
                Some(joined) = set.join_next() => {
                    match joined {
                        Ok(Ok(())) => {}
                        Ok(Err(e)) => {
                            result = Err(e);
                        }
                        Err(e) => {
                            result = Err(anyhow!("sync task join failed: {e}"));
                        }
                    }
                }
            }
        }
        while let Some(prog) = progress_rx.recv().await {
            on_progress(prog);
        }

        let total_items = items_lock.load(std::sync::atomic::Ordering::Relaxed);
        let final_accounts = progress_lock.lock().unwrap().clone();

        if result.is_ok() {
            let done = metadata::SyncProgress {
                active: false,
                percent: 100.0,
                items_indexed: total_items,
                message: if shallow {
                    format!("Ready · {total_items} top-level items")
                } else if total_items == 0 {
                    "Sync complete (no files found in My Drive / shared drives)".into()
                } else {
                    format!("Sync complete · {total_items} items")
                },
                account_total,
                accounts: final_accounts.clone(),
                ..Default::default()
            };
            on_progress(done.clone());
            self.mount.set_sync_progress(Some(done));
            self.mount.set_error(None);
        }

        self.mount.set_indexing(false);
        if let Err(e) = &result {
            self.mount.set_error(Some(e.to_string()));
            on_progress(metadata::SyncProgress {
                active: false,
                percent: 0.0,
                message: e.to_string(),
                account_total,
                accounts: final_accounts,
                ..Default::default()
            });
        }
        self.mount.set_sync_progress(None);
        result
    }

    pub async fn ensure_folder_children(&self, virtual_path: &str) -> Result<u64> {
        self.reload_providers()?;
        let path = metadata::normalize_virtual_path(virtual_path);
        let providers = self.providers.read().clone();
        if providers.is_empty() {
            return Err(anyhow!("No connected Google account"));
        }

        if path == "\\" {
            let mut total = 0u64;
            for p in providers {
                total += metadata::ensure_folder_children(&self.db, p.as_ref(), "\\").await?;
            }
            return Ok(total);
        }

        let node = self
            .db
            .get_node_by_path(&path)?
            .ok_or_else(|| anyhow!("Folder not found: {path}"))?;
        if !node.is_directory {
            return Ok(0);
        }
        let provider = providers
            .into_iter()
            .find(|p| p.account_id() == node.account_id)
            .ok_or_else(|| anyhow!("Provider not loaded for this folder"))?;
        metadata::ensure_folder_children(&self.db, provider.as_ref(), &path).await
    }

    pub fn read_file_preview(&self, virtual_path: &str, max_bytes: u64) -> Result<FilePreviewDto> {
        let path = metadata::normalize_virtual_path(virtual_path);
        let node = self
            .db
            .get_node_by_path(&path)?
            .ok_or_else(|| anyhow!("File not found: {path}"))?;
        if node.is_directory {
            return Err(anyhow!("Cannot preview a folder"));
        }
        let size = node.size.max(0) as u64;
        let want = max_bytes.min(size).min(40 * 1024 * 1024);
        let fs = self.fs_shared();
        let data = if want == 0 {
            Vec::new()
        } else {
            fs.read_file_range(&node, 0, want)?
        };
        use base64::Engine;
        Ok(FilePreviewDto {
            name: node.name.clone(),
            mime_type: node.mime_type.clone(),
            size,
            account_id: node.account_id.clone(),
            virtual_path: node.virtual_path.clone(),
            data_base64: base64::engine::general_purpose::STANDARD.encode(&data),
            truncated: want < size,
            on_disk_bytes: 0,
        })
    }

    pub fn create_link_to_desktop(&self, virtual_path: &str) -> Result<String> {
        let desktop = directories::UserDirs::new()
            .and_then(|u| u.desktop_dir().map(|p| p.to_path_buf()))
            .ok_or_else(|| anyhow!("Desktop folder not found"))?;
        self.create_link(virtual_path, &desktop.display().to_string())
    }

    pub fn mount_drive(&self, app: AppHandle) -> Result<DriveStatus> {
        if self.providers.read().is_empty() {
            self.reload_providers()?;
        }
        if self.providers.read().is_empty() {
            return Err(anyhow!("Connect a Google account before mounting"));
        }
        // Allow mount after a successful sync even if Drive is empty (root only).
        let indexed = self.db.any_account_indexed().unwrap_or(false);
        let node_count = self.db.count_nodes().unwrap_or(0);
        if !indexed && node_count <= 1 {
            return Err(anyhow!(
                "No indexed files yet. Click Sync metadata first and wait until it finishes."
            ));
        }
        self.mount.set_error(None);
        let fs = self.fs_shared();
        *self.app_for_fs.write() = Some(app);
        self.mount.mount(fs)
    }

    pub fn unmount_drive(&self) -> DriveStatus {
        self.mount.unmount()
    }

    /// Upload local files/folders dropped onto the Storbyte UI into `parent_path`.
    pub async fn upload_local_paths(
        &self,
        app: AppHandle,
        parent_path: &str,
        paths: Vec<String>,
    ) -> Result<u32> {
        if self.providers.read().is_empty() {
            self.reload_providers()?;
        }
        if self.providers.read().is_empty() {
            return Err(anyhow!("Connect a Google account before uploading"));
        }
        *self.app_for_fs.write() = Some(app);
        let fs = self.fs_shared();
        let mut total = 0u32;
        let mut errors: Vec<String> = Vec::new();
        for p in paths {
            let path = PathBuf::from(&p);
            if !path.exists() {
                errors.push(format!("missing: {p}"));
                continue;
            }
            match fs.upload_local_path(parent_path, &path).await {
                Ok(n) => total += n,
                Err(e) => errors.push(format!(
                    "{}: {e}",
                    path.file_name()
                        .and_then(|s| s.to_str())
                        .unwrap_or(p.as_str())
                )),
            }
        }
        if total == 0 && !errors.is_empty() {
            return Err(anyhow!(errors.join("\n")));
        }
        if !errors.is_empty() {
            tracing::warn!(?errors, uploaded = total, "partial upload errors");
        }
        Ok(total)
    }

    pub fn create_link(&self, virtual_path: &str, dest_dir: &str) -> Result<String> {
        if !self.settings.read().zero_bit_export {
            return Err(anyhow!(
                "Zero-bit export is OFF. Enable it in Settings to create link stubs."
            ));
        }
        let path = export::create_link_for_virtual_path(
            self.db.as_ref(),
            virtual_path,
            &PathBuf::from(dest_dir),
        )?;
        Ok(path.display().to_string())
    }

    pub fn open_link(&self, path: &str) -> Result<()> {
        export::open_link(self.db.as_ref(), &PathBuf::from(path))
    }

    /// Poll Google Drive Changes API in the background and emit `drive-changed` when metadata updates.
    pub fn start_change_watcher(&self, app: AppHandle) {
        if self.change_watch_started.swap(true, Ordering::SeqCst) {
            return;
        }
        self.runtime.spawn(async move {
            // First pass after a short delay so intro/shallow sync can seed tokens.
            tokio::time::sleep(Duration::from_secs(8)).await;
            loop {
                let applied = {
                    let Some(state) = app.try_state::<AppState>() else {
                        tokio::time::sleep(Duration::from_secs(25)).await;
                        continue;
                    };
                    state.poll_drive_changes_once().await
                };
                match applied {
                    Ok(n) if n > 0 => {
                        tracing::info!(applied = n, "drive changes applied");
                        let _ = app.emit(
                            "drive-changed",
                            serde_json::json!({ "applied": n }),
                        );
                    }
                    Ok(_) => {}
                    Err(e) => {
                        tracing::debug!(error = %e, "drive change poll failed");
                    }
                }
                tokio::time::sleep(Duration::from_secs(25)).await;
            }
        });
    }

    pub async fn poll_drive_changes_once(&self) -> Result<u64> {
        self.reload_providers()?;
        let providers = self.providers.read().clone();
        if providers.is_empty() {
            return Ok(0);
        }
        let mut total = 0u64;
        for p in providers {
            match metadata::apply_drive_changes(&self.db, p.as_ref()).await {
                Ok(n) => total += n,
                Err(e) => {
                    tracing::debug!(
                        account = %p.account_id(),
                        error = %e,
                        "apply_drive_changes failed"
                    );
                }
            }
        }
        Ok(total)
    }
}

#[allow(dead_code)]
fn _token_set(_: TokenSet) {}

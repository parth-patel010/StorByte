mod host;
mod security;

pub use host::{DriveStatus, MountHandle, MountManager};

use crate::blocks::{block_byte_range, blocks_for_range, block_size_bytes, first_byte_bytes};
use crate::cache::BlockCache;
use crate::config::{AppSettings, UploadPlacement};
use crate::export::{link_from_node, StorbyteLink};
use crate::metadata::{self, normalize_virtual_path, MetadataDb, NodeRow};
use crate::prefetch::PrefetchEngine;
use crate::providers::StorageProvider;
use crate::scheduler::{Priority, RequestScheduler};
use crate::stream_seal::prefetch_inflight_cap;
use bytes::Bytes;
use parking_lot::{Condvar, Mutex, RwLock};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter, Manager};
use tokio::runtime::Handle;

/// Coordinates "Ask every time" so Explorer retries / multi-file copies don't spam pickers.
pub struct UploadAskGate {
    /// UI reply channel for the active picker (if any).
    pick_tx: Mutex<Option<Sender<Option<String>>>>,
    /// Shared outcome of the in-flight ask: None while waiting, Some(choice) when done.
    outcome: Mutex<Option<Option<String>>>,
    cv: Condvar,
    active: AtomicBool,
    /// After Cancel, skip new pickers briefly (Explorer retries create immediately).
    suppress_until: Mutex<Option<Instant>>,
}

impl UploadAskGate {
    pub fn new() -> Self {
        Self {
            pick_tx: Mutex::new(None),
            outcome: Mutex::new(None),
            cv: Condvar::new(),
            active: AtomicBool::new(false),
            suppress_until: Mutex::new(None),
        }
    }

    pub fn is_suppressed(&self) -> bool {
        match *self.suppress_until.lock() {
            Some(until) => Instant::now() < until,
            None => false,
        }
    }

    fn suppress_for(&self, dur: Duration) {
        *self.suppress_until.lock() = Some(Instant::now() + dur);
    }

    pub fn resolve(&self, choice: Option<String>) -> bool {
        if choice.is_none() {
            // Cancel: stop Explorer from re-opening the picker on retries.
            self.suppress_for(Duration::from_secs(45));
        }
        let sent = if let Some(tx) = self.pick_tx.lock().take() {
            tx.send(choice.clone()).is_ok()
        } else {
            false
        };
        {
            let mut out = self.outcome.lock();
            *out = Some(choice);
        }
        self.active.store(false, Ordering::SeqCst);
        self.cv.notify_all();
        sent
    }

    /// Run or join a single ask session. Returns chosen account id, or None if cancelled/suppressed.
    pub fn ask(
        &self,
        app: Option<AppHandle>,
        file_name: &str,
        virtual_path: &str,
    ) -> Option<String> {
        if self.is_suppressed() {
            return None;
        }

        // Join an in-flight ask instead of opening another modal.
        if self.active.load(Ordering::SeqCst) {
            let mut out = self.outcome.lock();
            let wait = Duration::from_secs(120);
            let start = Instant::now();
            while out.is_none() {
                let left = wait.saturating_sub(start.elapsed());
                if left.is_zero() {
                    break;
                }
                let _ = self.cv.wait_for(&mut out, left);
            }
            return out.clone().flatten();
        }

        // We are the leader for this ask.
        if self
            .active
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .is_err()
        {
            // Lost race — join.
            return self.ask(app, file_name, virtual_path);
        }

        *self.outcome.lock() = None;

        let Some(app) = app else {
            self.active.store(false, Ordering::SeqCst);
            self.cv.notify_all();
            return None;
        };

        let (tx, rx): (Sender<Option<String>>, Receiver<Option<String>>) = mpsc::channel();
        *self.pick_tx.lock() = Some(tx);

        if let Some(win) = app.get_webview_window("main") {
            let _ = win.show();
            let _ = win.unminimize();
            let _ = win.set_focus();
        }
        let _ = app.emit(
            "upload-account-pick",
            serde_json::json!({
                "name": file_name,
                "path": virtual_path,
            }),
        );

        let choice = match rx.recv_timeout(Duration::from_secs(120)) {
            Ok(c) => c,
            Err(_) => {
                let _ = self.pick_tx.lock().take();
                // Timeout: treat as cancel + short suppress so retries don't stack modals.
                self.suppress_for(Duration::from_secs(15));
                None
            }
        };

        if choice.is_none() {
            self.suppress_for(Duration::from_secs(45));
            let _ = app.emit("upload-account-pick-cancelled", ());
        }

        {
            let mut out = self.outcome.lock();
            *out = Some(choice.clone());
        }
        self.active.store(false, Ordering::SeqCst);
        self.cv.notify_all();
        choice
    }
}

pub struct FsShared {
    pub db: Arc<MetadataDb>,
    pub settings: Arc<RwLock<AppSettings>>,
    pub cache: Arc<BlockCache>,
    pub prefetch: Arc<PrefetchEngine>,
    pub scheduler: Arc<RequestScheduler>,
    pub providers: Arc<RwLock<Vec<Arc<dyn StorageProvider>>>>,
    pub runtime: Handle,
    /// Filled when mounting so create_path can emit UI pick events.
    pub app: Arc<RwLock<Option<AppHandle>>>,
    pub upload_ask: Arc<UploadAskGate>,
}

impl FsShared {
    pub fn provider_for(&self, account_id: &str) -> Option<Arc<dyn StorageProvider>> {
        self.providers
            .read()
            .iter()
            .find(|p| p.account_id() == account_id)
            .cloned()
    }

    /// Stream a byte range via block-aligned cache + Google Drive range GETs.
    ///
    /// Cold misses use a small first-byte Range GET so Explorer gets data quickly,
    /// then fill the rest of the 4 MiB block in the background (Prefetch priority).
    pub fn read_file_range(&self, node: &NodeRow, offset: u64, length: u64) -> anyhow::Result<Vec<u8>> {
        if length == 0 {
            return Ok(Vec::new());
        }
        let file_size = node.size.max(0) as u64;
        if offset >= file_size {
            return Ok(Vec::new());
        }
        let length = length.min(file_size - offset);

        let provider = self
            .provider_for(&node.account_id)
            .ok_or_else(|| anyhow::anyhow!("provider not loaded for {}", node.account_id))?;

        let remote_id = node.remote_id.clone();
        let cache = self.cache.clone();
        let file_id = remote_id.clone();
        let spans = blocks_for_range(offset, length);
        let rt = self.runtime.clone();

        let mut out = Vec::with_capacity(length as usize);
        let mut last_block_index: Option<u64> = None;

        for span in spans {
            let (block_start, block_len) = block_byte_range(span.index, file_size);
            if block_len == 0 {
                break;
            }

            let slice = if let Some(hit) =
                cache.get_range(&file_id, span.index, span.offset_in_block, span.length)
            {
                hit
            } else {
                // Fast path: demand-fetch only what Explorer needs (capped), then
                // fill the full block asynchronously at Prefetch priority.
                let abs_off = block_start + span.offset_in_block;
                let remain_in_block = block_len.saturating_sub(span.offset_in_block);
                let first_len = first_byte_bytes()
                    .max(span.length)
                    .min(remain_in_block);

                let provider_fb = provider.clone();
                let remote_fb = remote_id.clone();
                let first: Bytes = rt
                    .block_on(async move {
                        tokio::time::timeout(
                            Duration::from_secs(120),
                            provider_fb.read_range(
                                &remote_fb,
                                abs_off,
                                first_len,
                                Priority::UserRead,
                            ),
                        )
                        .await
                        .map_err(|_| anyhow::anyhow!("timed out reading span {}", span.index))?
                    })?
                    .into();

                // Cache as block prefix when the fetch started at block start.
                if span.offset_in_block == 0 {
                    cache.put_prefix(&file_id, span.index, first.clone());
                }

                self.spawn_fill_block(
                    provider.clone(),
                    remote_id.clone(),
                    file_id.clone(),
                    span.index,
                    block_start,
                    block_len,
                );

                // If the request needs more than the first-byte window, finish
                // synchronously with a full-block UserRead (rare for Explorer).
                if span.length > first.len() as u64 {
                    let provider_full = provider.clone();
                    let remote_full = remote_id.clone();
                    let full: Bytes = rt
                        .block_on(async move {
                            tokio::time::timeout(
                                Duration::from_secs(120),
                                provider_full.read_range(
                                    &remote_full,
                                    block_start,
                                    block_len,
                                    Priority::UserRead,
                                ),
                            )
                            .await
                            .map_err(|_| {
                                anyhow::anyhow!("timed out reading block {}", span.index)
                            })?
                        })?
                        .into();
                    cache.put(&file_id, span.index, full.clone());
                    let start = span.offset_in_block as usize;
                    let end = (start + span.length as usize).min(full.len());
                    full.slice(start..end)
                } else {
                    let take = (span.length as usize).min(first.len());
                    first.slice(0..take)
                }
            };

            out.extend_from_slice(&slice);
            last_block_index = Some(span.index);
        }

        if let Some(idx) = last_block_index {
            let window = self.prefetch.observe(&file_id, idx);
            self.spawn_ahead_prefetch(
                provider,
                remote_id,
                file_id,
                file_size,
                idx + 1,
                window,
            );
        }

        Ok(out)
    }

    /// Background: replace a prefix / miss with a complete 4 MiB block.
    fn spawn_fill_block(
        &self,
        provider: Arc<dyn StorageProvider>,
        remote_id: String,
        file_id: String,
        index: u64,
        block_start: u64,
        block_len: u64,
    ) {
        if block_len == 0 || self.cache.contains_complete(&file_id, index) {
            return;
        }
        let cache = self.cache.clone();
        self.runtime.spawn(async move {
            if cache.contains_complete(&file_id, index) {
                return;
            }
            match provider
                .read_range(&remote_id, block_start, block_len, Priority::Prefetch)
                .await
            {
                Ok(data) => cache.put(&file_id, index, data),
                Err(e) => {
                    tracing::debug!(error = %e, block = index, "block fill miss");
                }
            }
        });
    }

    /// Prefetch the next `window` blocks after `start_index` (parallel, capped).
    fn spawn_ahead_prefetch(
        &self,
        provider: Arc<dyn StorageProvider>,
        remote_id: String,
        file_id: String,
        file_size: u64,
        start_index: u64,
        window: u32,
    ) {
        if window == 0 || start_index * block_size_bytes() >= file_size {
            return;
        }
        let cache = self.cache.clone();
        let in_flight = (window as usize).min(prefetch_inflight_cap());
        self.runtime.spawn(async move {
            let mut tasks = Vec::new();
            for i in 0..window {
                let index = start_index + u64::from(i);
                if cache.contains_complete(&file_id, index) {
                    continue;
                }
                let (block_start, block_len) = block_byte_range(index, file_size);
                if block_len == 0 {
                    break;
                }
                let provider = provider.clone();
                let remote_id = remote_id.clone();
                let cache = cache.clone();
                let file_id = file_id.clone();
                tasks.push(tokio::spawn(async move {
                    match provider
                        .read_range(&remote_id, block_start, block_len, Priority::Prefetch)
                        .await
                    {
                        Ok(data) => {
                            cache.put(&file_id, index, data);
                            Ok(())
                        }
                        Err(e) => {
                            tracing::debug!(error = %e, block = index, "ahead prefetch miss");
                            Err(e)
                        }
                    }
                }));
                if tasks.len() >= in_flight {
                    // Drain a batch before scheduling more.
                    for t in tasks.drain(..) {
                        let _ = t.await;
                    }
                }
            }
            for t in tasks {
                let _ = t.await;
            }
        });
    }

    /// Handle closed: keep RAM blocks (LRU eviction only) so reopen stays warm.
    pub fn release_stream_cache(&self, node: &NodeRow) {
        self.prefetch.reset_file(&node.remote_id);
    }

    pub fn resolve_upload_pick(&self, account_id: Option<String>) -> bool {
        self.upload_ask.resolve(account_id)
    }

    fn free_space(account: &crate::metadata::AccountRow) -> i64 {
        match (account.quota_bytes, account.usage_bytes) {
            (Some(q), Some(u)) => q.saturating_sub(u),
            (Some(q), None) => q,
            _ => 0,
        }
    }

    /// Pick account for a new item under `parent` (root uses upload_placement).
    /// When placement is Ask, shows the account picker (and brings Storbyte to the front).
    pub fn pick_upload_provider(
        &self,
        parent: &NodeRow,
        file_name: &str,
        virtual_path: &str,
        _allow_ask: bool,
    ) -> Option<Arc<dyn StorageProvider>> {
        let providers = self.providers.read().clone();
        if providers.is_empty() {
            return None;
        }

        // Nested folders always stay on the parent's account.
        if parent.virtual_path != "\\" {
            return providers
                .into_iter()
                .find(|p| p.account_id() == parent.account_id);
        }

        let placement = self.settings.read().upload_placement.clone();
        let mut accounts = self.db.list_accounts().unwrap_or_default();
        let needs_quota = accounts.iter().any(|a| {
            a.status == "connected" && (a.quota_bytes.is_none() || a.usage_bytes.is_none())
        });
        if needs_quota {
            let providers = providers.clone();
            let db = self.db.clone();
            let _ = self.runtime.block_on(async {
                tokio::time::timeout(Duration::from_secs(8), async move {
                    for p in providers {
                        if let Ok(Some(q)) = p.about_quota().await {
                            if let Ok(mut list) = db.list_accounts() {
                                if let Some(a) = list.iter_mut().find(|a| a.id == p.account_id()) {
                                    a.quota_bytes = q.limit_bytes.map(|v| v as i64);
                                    a.usage_bytes = q.usage_bytes.map(|v| v as i64);
                                    let _ = db.upsert_account(a);
                                }
                            }
                        }
                    }
                })
                .await
            });
            accounts = self.db.list_accounts().unwrap_or_default();
        }
        let connected: Vec<_> = accounts
            .into_iter()
            .filter(|a| a.status == "connected")
            .collect();

        let by_free = |most: bool| -> Option<String> {
            let mut best: Option<(i64, String)> = None;
            for a in &connected {
                let free = Self::free_space(a);
                match &best {
                    None => best = Some((free, a.id.clone())),
                    Some((bf, _)) => {
                        if (most && free > *bf) || (!most && free < *bf) {
                            best = Some((free, a.id.clone()));
                        }
                    }
                }
            }
            best.map(|(_, id)| id)
        };

        let account_id = match placement {
            UploadPlacement::MostFree => by_free(true),
            UploadPlacement::LeastFree => by_free(false),
            UploadPlacement::Ask => {
                // Cancel / suppress must not fall through to a default account.
                return match self
                    .upload_ask
                    .ask(self.app.read().clone(), file_name, virtual_path)
                {
                    Some(id) => providers.into_iter().find(|p| p.account_id() == id),
                    None => None,
                };
            }
        };

        if let Some(id) = account_id {
            if let Some(p) = providers.iter().find(|p| p.account_id() == id) {
                return Some(p.clone());
            }
        }
        // Fallback: settings default, then first provider.
        let settings = self.settings.read();
        if let Some(id) = &settings.default_account_id {
            if let Some(p) = providers.iter().find(|p| p.account_id() == id) {
                return Some(p.clone());
            }
        }
        providers.first().cloned()
    }

    /// If folder children are stale, fetch this folder from Drive (bounded wait).
    pub fn ensure_directory_if_stale(&self, path: &str, max_age_secs: i64, timeout_ms: u64) {
        let path = normalize_virtual_path(path);
        if !self
            .db
            .folder_children_stale(&path, max_age_secs)
            .unwrap_or(true)
        {
            return;
        }

        let providers = self.providers.read().clone();
        if providers.is_empty() {
            return;
        }
        let db = self.db.clone();
        let path2 = path.clone();
        let fut = async move {
            if path2 == "\\" {
                for p in providers {
                    if let Err(e) = metadata::ensure_folder_children(&db, p.as_ref(), "\\").await {
                        tracing::debug!(error = %e, "ensure root children failed");
                    }
                }
            } else if let Ok(Some(node)) = db.get_node_by_path(&path2) {
                if let Some(p) = providers
                    .into_iter()
                    .find(|p| p.account_id() == node.account_id)
                {
                    if let Err(e) =
                        metadata::ensure_folder_children(&db, p.as_ref(), &path2).await
                    {
                        tracing::debug!(error = %e, path = %path2, "ensure folder children failed");
                    }
                }
            }
        };
        let _ = self.runtime.block_on(async {
            tokio::time::timeout(Duration::from_millis(timeout_ms), fut).await
        });
    }

    /// Kick off open warm: await a short first-byte head, then prefetch more in background.
    pub fn prefetch_for_open(&self, node: &NodeRow) {
        if node.is_directory || node.size <= 0 {
            return;
        }
        let size = node.size as u64;
        let name = node.name.to_lowercase();
        let archive = [".zip", ".rar", ".7z", ".jar", ".apk", ".docx", ".xlsx", ".pptx"]
            .iter()
            .any(|ext| name.ends_with(ext));
        let media = [".mp4", ".mkv", ".mov", ".webm", ".avi", ".mp3", ".wav", ".pdf"]
            .iter()
            .any(|ext| name.ends_with(ext));

        let provider = match self.provider_for(&node.account_id) {
            Some(p) => p,
            None => return,
        };
        let cache = self.cache.clone();
        let remote_id = node.remote_id.clone();
        let file_id = remote_id.clone();
        let rt = self.runtime.clone();

        // Join open-warm: pull a small head with UserRead so the first Explorer
        // read often hits RAM (turbolite-style frontrun). Cap wait ~100ms.
        let head_warm = first_byte_bytes().min(size);
        if head_warm > 0 && cache.get_range(&file_id, 0, 0, head_warm).is_none() {
            let provider_h = provider.clone();
            let remote_h = remote_id.clone();
            let cache_h = cache.clone();
            let file_h = file_id.clone();
            let _ = rt.block_on(async move {
                match tokio::time::timeout(
                    Duration::from_millis(100),
                    provider_h.read_range(&remote_h, 0, head_warm, Priority::UserRead),
                )
                .await
                {
                    Ok(Ok(data)) => {
                        cache_h.put_prefix(&file_h, 0, data);
                        Ok(())
                    }
                    Ok(Err(e)) => {
                        tracing::debug!(error = %e, "open head warm failed");
                        Err(e)
                    }
                    Err(_) => {
                        tracing::debug!("open head warm timed out");
                        Ok(())
                    }
                }
            });
        }

        // Background: larger head/tail at Prefetch priority (archives need EOCD).
        let mut ranges: Vec<(u64, u64)> = Vec::new();
        let head = if archive {
            (8 * 1024 * 1024u64).min(size)
        } else if media {
            (2 * block_size_bytes()).min(size)
        } else {
            block_size_bytes().min(size)
        };
        ranges.push((0, head));
        if archive && size > head {
            let tail_len = (4 * 1024 * 1024u64).min(size);
            let tail_start = size.saturating_sub(tail_len);
            if tail_start >= head {
                ranges.push((tail_start, tail_len));
            }
        }

        rt.spawn(async move {
            let mut tasks = Vec::new();
            for (off, len) in ranges {
                for span in blocks_for_range(off, len) {
                    if cache.contains_complete(&file_id, span.index) {
                        continue;
                    }
                    let (block_start, block_len) = block_byte_range(span.index, size);
                    if block_len == 0 {
                        continue;
                    }
                    let provider = provider.clone();
                    let remote_id = remote_id.clone();
                    let cache = cache.clone();
                    let file_id = file_id.clone();
                    tasks.push(tokio::spawn(async move {
                        match provider
                            .read_range(
                                &remote_id,
                                block_start,
                                block_len,
                                Priority::Prefetch,
                            )
                            .await
                        {
                            Ok(data) => cache.put(&file_id, span.index, data),
                            Err(e) => {
                                tracing::debug!(error = %e, block = span.index, "prefetch miss");
                            }
                        }
                    }));
                }
            }
            for t in tasks {
                let _ = t.await;
            }
        });
    }

    pub fn storbyte_link_for(&self, node: &NodeRow) -> StorbyteLink {
        link_from_node(
            &node.account_id,
            &node.remote_id,
            &node.name,
            node.size as u64,
            node.mime_type.clone(),
            &node.virtual_path,
        )
    }

    /// Upload a local file/folder into `parent_vpath` (Drive + SQLite metadata).
    pub async fn upload_local_path(
        &self,
        parent_vpath: &str,
        local: &std::path::Path,
    ) -> anyhow::Result<u32> {
        const MAX_BYTES: u64 = 250 * 1024 * 1024;
        let parent_vpath = normalize_virtual_path(parent_vpath);
        let meta = tokio::fs::metadata(local).await?;
        let name = local
            .file_name()
            .and_then(|s| s.to_str())
            .ok_or_else(|| anyhow::anyhow!("invalid file name"))?
            .to_string();
        let dest = if parent_vpath == "\\" {
            format!("\\{name}")
        } else {
            format!("{parent_vpath}\\{name}")
        };

        let parent = match self.db.get_node_by_path(&parent_vpath)? {
            Some(p) => p,
            None if parent_vpath == "\\" => {
                let account_id = self
                    .providers
                    .read()
                    .first()
                    .map(|p| p.account_id().to_string())
                    .ok_or_else(|| anyhow::anyhow!("no connected account for uploads"))?;
                self.db.ensure_root(&account_id)?;
                self.db
                    .get_node_by_path("\\")?
                    .ok_or_else(|| anyhow::anyhow!("folder missing: \\"))?
            }
            None => {
                return Err(anyhow::anyhow!("folder missing: {parent_vpath}"));
            }
        };

        if meta.is_dir() {
            let provider = self
                .pick_upload_provider(&parent, &name, &dest, true)
                .ok_or_else(|| anyhow::anyhow!("Upload cancelled"))?;
            let parent_remote = if parent.virtual_path == "\\" {
                "root".to_string()
            } else {
                parent.remote_id.clone()
            };

            // Prefer an existing Drive/local folder with this name (avoid Name (XXXXXXXX)).
            let folder_path = if let Some(existing) = self.db.find_child_by_name(
                parent.id,
                &name,
                true,
                Some(provider.account_id()),
            )? {
                existing.virtual_path
            } else {
                let children = provider.list_children(&parent_remote).await?;
                let stat = if let Some(hit) = children
                    .iter()
                    .find(|c| c.is_directory && c.name.eq_ignore_ascii_case(&name))
                {
                    crate::providers::RemoteStat {
                        id: hit.id.clone(),
                        name: hit.name.clone(),
                        is_directory: true,
                        size: hit.size,
                        mime_type: hit.mime_type.clone(),
                        modified_time: hit.modified_time.clone(),
                        md5: hit.md5.clone(),
                    }
                } else {
                    provider
                        .create_directory(&parent_remote, &name)
                        .await?
                };
                let (_id, folder_path) = self.db.insert_node(
                    Some(parent.id),
                    &name,
                    &dest,
                    true,
                    0,
                    Some("application/vnd.google-apps.folder"),
                    &stat.id,
                    provider.account_id(),
                    stat.modified_time.as_deref(),
                    None,
                )?;
                folder_path
            };
            let mut count = 1u32;
            let mut entries = tokio::fs::read_dir(local).await?;
            while let Some(entry) = entries.next_entry().await? {
                count += Box::pin(self.upload_local_path(&folder_path, &entry.path())).await?;
            }
            return Ok(count);
        }

        let size = meta.len();
        if size > MAX_BYTES {
            anyhow::bail!(
                "\"{name}\" is too large for in-app drop (max 250 MB). Drop it onto the mounted drive letter instead."
            );
        }
        let data = tokio::fs::read(local).await?;
        let provider = self
            .pick_upload_provider(&parent, &name, &dest, true)
            .ok_or_else(|| anyhow::anyhow!("Upload cancelled"))?;
        let parent_remote = if parent.virtual_path == "\\" {
            "root".to_string()
        } else {
            parent.remote_id.clone()
        };
        let created = provider
            .create_file(&parent_remote, &name, None)
            .await?;
        let written = provider.write_replace(&created.id, &data).await?;
        self.db.insert_node(
            Some(parent.id),
            &name,
            &dest,
            false,
            data.len() as i64,
            written.mime_type.as_deref().or(created.mime_type.as_deref()),
            &written.id,
            provider.account_id(),
            written.modified_time.as_deref(),
            None,
        )?;
        Ok(1)
    }
}

pub fn path_from_winfsp(name: &str) -> String {
    normalize_virtual_path(name)
}

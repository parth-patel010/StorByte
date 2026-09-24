use super::security::default_security_descriptor;
use super::{path_from_winfsp, FsShared};
use crate::metadata::NodeRow;
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use std::ffi::c_void;
use std::sync::Arc;
use std::thread::{self, JoinHandle};
use std::time::{SystemTime, UNIX_EPOCH};
use winfsp::filesystem::{
    DirInfo, DirMarker, FileInfo, FileSecurity, FileSystemContext, OpenFileInfo, VolumeInfo,
    WideNameInfo,
};
use winfsp::host::{FileSystemHost, FileSystemParams, VolumeParams, DebugMode};
use winfsp::{FspError, U16CStr};
use windows::Win32::Foundation::{
    STATUS_END_OF_FILE, STATUS_FILE_IS_A_DIRECTORY, STATUS_OBJECT_NAME_COLLISION,
    STATUS_OBJECT_NAME_NOT_FOUND,
};
use windows::Win32::Storage::FileSystem::{FILE_ATTRIBUTE_DIRECTORY, FILE_ATTRIBUTE_NORMAL};

const SECTOR_SIZE: u16 = 4096;
const SECTORS_PER_ALLOCATION_UNIT: u16 = 1;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DriveStatus {
    pub mounted: bool,
    pub drive_letter: String,
    pub indexing: bool,
    pub last_error: Option<String>,
    pub sync: Option<crate::metadata::SyncProgress>,
    /// True while auto-mount is enabled but the PC is offline.
    #[serde(default)]
    pub waiting_for_network: bool,
}

pub struct MountHandle {
    stop: Arc<Mutex<bool>>,
    join: Option<JoinHandle<()>>,
}

impl MountHandle {
    pub fn stop(mut self) {
        *self.stop.lock() = true;
        if let Some(j) = self.join.take() {
            let _ = j.join();
        }
    }
}

pub struct MountManager {
    current: Mutex<Option<MountHandle>>,
    status: Mutex<DriveStatus>,
}

impl MountManager {
    pub fn new(drive_letter: String) -> Self {
        Self {
            current: Mutex::new(None),
            status: Mutex::new(DriveStatus {
                mounted: false,
                drive_letter,
                indexing: false,
                last_error: None,
                sync: None,
                waiting_for_network: false,
            }),
        }
    }

    pub fn status(&self) -> DriveStatus {
        self.status.lock().clone()
    }

    pub fn set_drive_letter(&self, letter: String) {
        self.status.lock().drive_letter = letter;
    }

    pub fn set_waiting_for_network(&self, waiting: bool) {
        self.status.lock().waiting_for_network = waiting;
    }

    pub fn set_indexing(&self, indexing: bool) {
        let mut st = self.status.lock();
        st.indexing = indexing;
        if !indexing {
            st.sync = None;
        }
    }

    pub fn set_sync_progress(&self, progress: Option<crate::metadata::SyncProgress>) {
        let mut st = self.status.lock();
        st.sync = progress;
        st.indexing = st.sync.as_ref().map(|p| p.active).unwrap_or(st.indexing);
    }

    pub fn set_error(&self, err: Option<String>) {
        self.status.lock().last_error = err;
    }

    pub fn mount(&self, shared: Arc<FsShared>) -> anyhow::Result<DriveStatus> {
        // Already attached — do not tear down / remount (causes Explorer lag on reopen).
        if self.status().mounted && self.current.lock().is_some() {
            return Ok(self.status());
        }

        self.unmount();

        // Never use winfsp_init_or_die — that calls process::exit(1285) and closes the whole app.
        ensure_winfsp()?;

        let letter = shared.settings.read().drive_letter.clone();
        let mount_point = format!("{letter}:");
        let stop = Arc::new(Mutex::new(false));
        let stop_flag = stop.clone();
        let status_letter = letter.clone();
        let (ready_tx, ready_rx) = std::sync::mpsc::channel::<Result<(), String>>();

        let join = thread::Builder::new()
            .name("storbyte-winfsp".into())
            .spawn(move || {
                if let Err(e) = run_host(shared, &mount_point, stop_flag, ready_tx) {
                    tracing::error!(error = %e, "WinFsp host exited with error");
                }
            })?;

        match ready_rx.recv_timeout(std::time::Duration::from_secs(15)) {
            Ok(Ok(())) => {}
            Ok(Err(e)) => {
                let _ = join.join();
                self.status.lock().mounted = false;
                self.status.lock().last_error = Some(e.clone());
                anyhow::bail!("{e}");
            }
            Err(_) => {
                *stop.lock() = true;
                let _ = join.join();
                self.status.lock().mounted = false;
                anyhow::bail!("Timed out mounting {status_letter}: — is the drive letter free?");
            }
        }

        {
            let mut st = self.status.lock();
            st.mounted = true;
            st.drive_letter = status_letter;
            st.last_error = None;
        }
        *self.current.lock() = Some(MountHandle {
            stop,
            join: Some(join),
        });
        Ok(self.status())
    }

    pub fn unmount(&self) -> DriveStatus {
        if let Some(handle) = self.current.lock().take() {
            handle.stop();
        }
        let mut st = self.status.lock();
        st.mounted = false;
        st.waiting_for_network = false;
        st.clone()
    }
}

fn ensure_winfsp() -> anyhow::Result<winfsp::FspInit> {
    if let Ok(token) = winfsp::winfsp_init() {
        return Ok(token);
    }

    // Local PATH often lacks WinFsp; load the installed DLL explicitly then retry.
    #[cfg(windows)]
    {
        use std::path::Path;
        use windows::core::PCWSTR;
        use windows::Win32::System::LibraryLoader::LoadLibraryW;

        let candidates = [
            r"C:\Program Files (x86)\WinFsp\bin\winfsp-x64.dll",
            r"C:\Program Files\WinFsp\bin\winfsp-x64.dll",
        ];
        for path in candidates {
            if !Path::new(path).exists() {
                continue;
            }
            let wide: Vec<u16> = path.encode_utf16().chain(std::iter::once(0)).collect();
            let loaded = unsafe { LoadLibraryW(PCWSTR(wide.as_ptr())) };
            if loaded.is_ok() {
                if let Ok(token) = winfsp::winfsp_init() {
                    return Ok(token);
                }
            }
        }
    }

    anyhow::bail!(
        "WinFsp could not be loaded. Install WinFsp from https://winfsp.dev/rel/ (x64), then restart Storbyte."
    )
}

fn run_host(
    shared: Arc<FsShared>,
    mount_point: &str,
    stop: Arc<Mutex<bool>>,
    ready: std::sync::mpsc::Sender<Result<(), String>>,
) -> anyhow::Result<()> {
    let _token = match ensure_winfsp() {
        Ok(t) => t,
        Err(e) => {
            let msg = e.to_string();
            let _ = ready.send(Err(msg.clone()));
            return Err(anyhow::anyhow!(msg));
        }
    };

    let security = match default_security_descriptor() {
        Ok(s) => s,
        Err(e) => {
            let msg = format!("security descriptor: {e}");
            let _ = ready.send(Err(msg.clone()));
            return Err(anyhow::anyhow!(msg));
        }
    };
    let context = StorbyteFs {
        shared,
        security,
    };

    let mut volume_params = VolumeParams::new();
    volume_params
        .sector_size(SECTOR_SIZE)
        .sectors_per_allocation_unit(SECTORS_PER_ALLOCATION_UNIT)
        .volume_creation_time(filetime_now())
        .volume_serial_number(0x53545258) // STRX
        // Keep dir/file attrs warm, but refresh volume capacity often so account
        // remove/add updates Explorer free/total space.
        .file_info_timeout(1_000)
        .volume_info_timeout(500)
        .case_sensitive_search(false)
        .case_preserved_names(true)
        .unicode_on_disk(true)
        .persistent_acls(true)
        .post_cleanup_when_modified_only(true)
        .flush_and_purge_on_cleanup(true)
        .allow_open_in_kernel_mode(true)
        .filesystem_name("Storbyte");

    let params = FileSystemParams {
        use_dir_info_by_name: false,
        volume_params,
        debug_mode: DebugMode::none(),
    };

    let mut host = match FileSystemHost::<StorbyteFs>::new_with_options(params, context) {
        Ok(h) => h,
        Err(e) => {
            let msg = format!("WinFsp host create failed: {e}");
            let _ = ready.send(Err(msg.clone()));
            return Err(anyhow::anyhow!(msg));
        }
    };
    if let Err(e) = host.mount(mount_point) {
        let msg = format!(
            "Could not mount {mount_point} ({e}). Pick a free drive letter in Settings."
        );
        let _ = ready.send(Err(msg.clone()));
        return Err(anyhow::anyhow!(msg));
    }
    if let Err(e) = host.start() {
        let _ = host.unmount();
        let msg = format!("WinFsp start failed: {e}");
        let _ = ready.send(Err(msg.clone()));
        return Err(anyhow::anyhow!(msg));
    }
    let _ = ready.send(Ok(()));
    tracing::info!(mount_point, "Storbyte drive mounted");

    while !*stop.lock() {
        thread::sleep(std::time::Duration::from_millis(200));
    }

    host.stop();
    host.unmount();
    tracing::info!("Storbyte drive unmounted");
    Ok(())
}

struct StorbyteFs {
    shared: Arc<FsShared>,
    security: Vec<u8>,
}

pub struct FileCtx {
    path: Mutex<String>,
    node: Mutex<Option<NodeRow>>,
    is_root: bool,
    is_directory: bool,
    dir_entries: Mutex<Option<Vec<NodeRow>>>,
    write_buf: Mutex<Vec<u8>>,
    dirty: Mutex<bool>,
    delete_pending: Mutex<bool>,
}

fn filetime_now() -> u64 {
    let dur = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    dur.as_secs() * 10_000_000 + (dur.subsec_nanos() as u64) / 100 + 116_444_736_000_000_000
}

fn parse_time(s: &Option<String>) -> u64 {
    s.as_ref()
        .and_then(|t| chrono::DateTime::parse_from_rfc3339(t).ok())
        .map(|dt| {
            let ts = dt.timestamp();
            let nanos = dt.timestamp_subsec_nanos();
            (ts as u64) * 10_000_000 + (nanos as u64) / 100 + 116_444_736_000_000_000
        })
        .unwrap_or_else(filetime_now)
}

fn fill_info(node: &NodeRow, info: &mut FileInfo) {
    let is_dir = node.is_directory;
    // Do NOT set OFFLINE / RECALL_ON_DATA_ACCESS — those make Explorer try to
    // "hydrate" archives (ZIPs) synchronously and freeze the shell.
    //
    // Content is streamed from Drive (RAM cache only) — nothing is written to V:\
    // clusters. We still report AllocationSize = 0 here for directory listings /
    // tools that pass it through, but WinFsp's FSD clamps AllocationSize >= FileSize
    // on open (cache manager), and Explorer Properties for normal files uses
    // FileSize rounded to the cluster — so Properties "Size on disk" will still
    // look ≈ Size. True OneDrive-style 0 requires Cloud Files placeholders, not WinFsp.
    let attrs = if is_dir {
        FILE_ATTRIBUTE_DIRECTORY.0
    } else {
        FILE_ATTRIBUTE_NORMAL.0
    };
    let now = parse_time(&node.modified_at);
    *info = FileInfo {
        file_attributes: attrs,
        reparse_tag: 0,
        allocation_size: 0,
        file_size: if is_dir { 0 } else { node.size.max(0) as u64 },
        creation_time: now,
        last_access_time: now,
        last_write_time: now,
        change_time: now,
        index_number: node.id as u64,
        hard_links: 0,
        ea_size: 0,
    };
}

fn fill_root(info: &mut FileInfo) {
    let now = filetime_now();
    *info = FileInfo {
        file_attributes: FILE_ATTRIBUTE_DIRECTORY.0,
        reparse_tag: 0,
        allocation_size: 0,
        file_size: 0,
        creation_time: now,
        last_access_time: now,
        last_write_time: now,
        change_time: now,
        index_number: 1,
        hard_links: 0,
        ea_size: 0,
    };
}

fn wide_to_string(name: &U16CStr) -> String {
    String::from_utf16_lossy(name.as_slice())
}

impl FileSystemContext for StorbyteFs {
    type FileContext = FileCtx;

    fn get_security_by_name(
        &self,
        file_name: &U16CStr,
        security_descriptor: Option<&mut [c_void]>,
        _reparse_point_resolver: impl FnOnce(&U16CStr) -> Option<FileSecurity>,
    ) -> winfsp::Result<FileSecurity> {
        let path = path_from_winfsp(&wide_to_string(file_name));
        let attrs = if path == "\\" {
            FILE_ATTRIBUTE_DIRECTORY.0
        } else {
            match self.shared.db.get_node_by_path(&path) {
                Ok(Some(n)) if n.is_directory => FILE_ATTRIBUTE_DIRECTORY.0,
                Ok(Some(_)) => FILE_ATTRIBUTE_NORMAL.0,
                Ok(None) => {
                    return Err(FspError::NTSTATUS(STATUS_OBJECT_NAME_NOT_FOUND.0));
                }
                Err(_) => return Err(FspError::NTSTATUS(STATUS_OBJECT_NAME_NOT_FOUND.0)),
            }
        };

        let security_size = self.security.len() as u64;
        if let Some(buffer) = security_descriptor {
            if (buffer.len() as u64) >= security_size {
                unsafe {
                    std::ptr::copy_nonoverlapping(
                        self.security.as_ptr(),
                        buffer.as_mut_ptr() as *mut u8,
                        self.security.len(),
                    );
                }
            }
        }

        Ok(FileSecurity {
            reparse: false,
            sz_security_descriptor: security_size,
            attributes: attrs,
        })
    }

    fn open(
        &self,
        file_name: &U16CStr,
        _create_options: u32,
        _granted_access: u32,
        file_info: &mut OpenFileInfo,
    ) -> winfsp::Result<Self::FileContext> {
        let path = path_from_winfsp(&wide_to_string(file_name));
        if path == "\\" {
            self.shared.ensure_directory_if_stale("\\", 120, 8_000);
            fill_root(file_info.as_mut());
            return Ok(FileCtx {
                path: Mutex::new(path),
                node: Mutex::new(None),
                is_root: true,
                is_directory: true,
                dir_entries: Mutex::new(None),
                write_buf: Mutex::new(Vec::new()),
                dirty: Mutex::new(false),
                delete_pending: Mutex::new(false),
            });
        }

        let node = self
            .shared
            .db
            .get_node_by_path(&path)
            .ok()
            .flatten()
            .ok_or_else(|| FspError::NTSTATUS(STATUS_OBJECT_NAME_NOT_FOUND.0))?;

        let is_directory = node.is_directory;
        if is_directory {
            self.shared
                .ensure_directory_if_stale(&path, 120, 8_000);
        } else {
            // Frontrun first-byte head (joined briefly), then background warm.
            self.shared.prefetch_for_open(&node);
        }
        fill_info(&node, file_info.as_mut());
        Ok(FileCtx {
            path: Mutex::new(path),
            node: Mutex::new(Some(node)),
            is_root: false,
            is_directory,
            dir_entries: Mutex::new(None),
            write_buf: Mutex::new(Vec::new()),
            dirty: Mutex::new(false),
            delete_pending: Mutex::new(false),
        })
    }

    fn close(&self, context: Self::FileContext) {
        let _ = self.flush_writes(&context);
        // Keep RAM blocks across reopen (LRU only); reset sequential prefetch state.
        if !context.is_directory && !context.is_root {
            if let Some(node) = context.node.lock().as_ref() {
                self.shared.release_stream_cache(node);
            }
        }
    }

    fn cleanup(
        &self,
        context: &Self::FileContext,
        _file_name: Option<&U16CStr>,
        flags: u32,
    ) {
        // FspCleanupDelete = 0x01
        if (flags & 0x01) != 0 || *context.delete_pending.lock() {
            if let Some(node) = context.node.lock().as_ref() {
                self.shared.release_stream_cache(node);
            }
            let _ = self.delete_path(&context.path.lock());
            return;
        }
        let _ = self.flush_writes(context);
    }

    fn get_file_info(
        &self,
        context: &Self::FileContext,
        file_info: &mut FileInfo,
    ) -> winfsp::Result<()> {
        if context.is_root {
            fill_root(file_info);
        } else if let Some(node) = context.node.lock().as_ref() {
            fill_info(node, file_info);
        } else {
            return Err(FspError::NTSTATUS(STATUS_OBJECT_NAME_NOT_FOUND.0));
        }
        Ok(())
    }

    fn read_directory(
        &self,
        context: &Self::FileContext,
        _pattern: Option<&U16CStr>,
        marker: DirMarker<'_>,
        buffer: &mut [u8],
    ) -> winfsp::Result<u32> {
        let path = if context.is_root {
            "\\".to_string()
        } else if let Some(n) = context.node.lock().as_ref() {
            n.virtual_path.clone()
        } else {
            context.path.lock().clone()
        };

        {
            let mut cache = context.dir_entries.lock();
            if cache.is_none() {
                self.shared
                    .ensure_directory_if_stale(&path, 120, 8_000);
                let entries = self
                    .shared
                    .db
                    .list_children(&path)
                    .unwrap_or_default();
                *cache = Some(entries);
            }
        }

        let entries = context.dir_entries.lock();
        let entries = entries.as_ref().cloned().unwrap_or_default();

        let mut cursor = 0u32;
        let mut dir_info: DirInfo<255> = DirInfo::new();
        let start = marker
            .inner_as_cstr()
            .map(|m| String::from_utf16_lossy(m.as_slice()))
            .unwrap_or_default();

        let mut past = start.is_empty();
        for ent in entries {
            if !past {
                if ent.name == start {
                    past = true;
                }
                continue;
            }
            dir_info.reset();
            fill_info(&ent, dir_info.file_info_mut());
            let wide: Vec<u16> = ent.name.encode_utf16().collect();
            if dir_info.set_name_raw(wide.as_slice()).is_err() {
                continue;
            }
            if !dir_info.append_to_buffer(buffer, &mut cursor) {
                break;
            }
        }
        Ok(cursor)
    }

    fn read(
        &self,
        context: &Self::FileContext,
        buffer: &mut [u8],
        offset: u64,
    ) -> winfsp::Result<u32> {
        if context.is_directory {
            return Err(FspError::NTSTATUS(STATUS_FILE_IS_A_DIRECTORY.0));
        }
        // Prefer dirty local write buffer when present.
        {
            let buf = context.write_buf.lock();
            if *context.dirty.lock() && !buf.is_empty() {
                if offset as usize >= buf.len() {
                    return Err(FspError::NTSTATUS(STATUS_END_OF_FILE.0));
                }
                let end = ((offset as usize) + buffer.len()).min(buf.len());
                let n = end - offset as usize;
                buffer[..n].copy_from_slice(&buf[offset as usize..end]);
                return Ok(n as u32);
            }
        }
        let node = context
            .node
            .lock()
            .clone()
            .ok_or_else(|| FspError::NTSTATUS(STATUS_OBJECT_NAME_NOT_FOUND.0))?;
        let size = node.size as u64;
        if offset >= size {
            return Err(FspError::NTSTATUS(STATUS_END_OF_FILE.0));
        }
        let want = (buffer.len() as u64).min(size - offset);
        match self.shared.read_file_range(&node, offset, want) {
            Ok(data) => {
                let n = data.len().min(buffer.len());
                buffer[..n].copy_from_slice(&data[..n]);
                Ok(n as u32)
            }
            Err(e) => {
                tracing::warn!(error = %e, path = %node.virtual_path, "read_range failed");
                Err(FspError::NTSTATUS(STATUS_OBJECT_NAME_NOT_FOUND.0))
            }
        }
    }

    fn create(
        &self,
        file_name: &U16CStr,
        create_options: u32,
        _granted_access: u32,
        _file_attributes: u32,
        _security_descriptor: Option<&[c_void]>,
        _allocation_size: u64,
        _extra_buffer: Option<&[u8]>,
        _extra_buffer_is_reparse_point: bool,
        file_info: &mut OpenFileInfo,
    ) -> winfsp::Result<Self::FileContext> {
        let path = path_from_winfsp(&wide_to_string(file_name));
        let is_directory = (create_options & 0x0000_0001) != 0; // FILE_DIRECTORY_FILE
        match self.create_path(&path, is_directory) {
            Ok(node) => {
                fill_info(&node, file_info.as_mut());
                Ok(FileCtx {
                    path: Mutex::new(path),
                    is_directory: node.is_directory,
                    node: Mutex::new(Some(node)),
                    is_root: false,
                    dir_entries: Mutex::new(None),
                    write_buf: Mutex::new(Vec::new()),
                    dirty: Mutex::new(false),
                    delete_pending: Mutex::new(false),
                })
            }
            Err(e) => {
                tracing::warn!(error = %e, %path, "create failed");
                Err(FspError::NTSTATUS(STATUS_OBJECT_NAME_NOT_FOUND.0))
            }
        }
    }

    fn write(
        &self,
        context: &Self::FileContext,
        buffer: &[u8],
        offset: u64,
        write_to_eof: bool,
        _constrained_io: bool,
        file_info: &mut FileInfo,
    ) -> winfsp::Result<u32> {
        if context.is_directory {
            return Err(FspError::NTSTATUS(STATUS_FILE_IS_A_DIRECTORY.0));
        }
        let mut buf = context.write_buf.lock();
        // Only seed the prefix before the write point (max one stream block).
        // Never pull the whole remote file into RAM on first write.
        if buf.is_empty() {
            if let Some(node) = context.node.lock().as_ref() {
                let write_at = if write_to_eof {
                    node.size.max(0) as u64
                } else {
                    offset
                };
                if write_at > 0 && node.size > 0 {
                    let seed_end = write_at
                        .min(node.size as u64)
                        .min(crate::blocks::block_size_bytes());
                    if let Ok(existing) = self.shared.read_file_range(node, 0, seed_end) {
                        *buf = existing;
                    }
                }
            }
        }
        let start = if write_to_eof {
            buf.len() as u64
        } else {
            offset
        };
        let end = start as usize + buffer.len();
        if end > buf.len() {
            buf.resize(end, 0);
        }
        buf[start as usize..end].copy_from_slice(buffer);
        *context.dirty.lock() = true;
        if let Some(node) = context.node.lock().as_mut() {
            node.size = buf.len() as i64;
            fill_info(node, file_info);
        } else {
            fill_root(file_info);
            file_info.file_size = buf.len() as u64;
            file_info.allocation_size = 0;
        }
        Ok(buffer.len() as u32)
    }

    fn flush(
        &self,
        context: Option<&Self::FileContext>,
        file_info: &mut FileInfo,
    ) -> winfsp::Result<()> {
        if let Some(ctx) = context {
            let _ = self.flush_writes(ctx);
            if let Some(node) = ctx.node.lock().as_ref() {
                fill_info(node, file_info);
            }
        }
        Ok(())
    }

    fn set_delete(
        &self,
        context: &Self::FileContext,
        _file_name: &U16CStr,
        delete_file: bool,
    ) -> winfsp::Result<()> {
        *context.delete_pending.lock() = delete_file;
        Ok(())
    }

    fn rename(
        &self,
        context: &Self::FileContext,
        _file_name: &U16CStr,
        new_file_name: &U16CStr,
        replace_if_exists: bool,
    ) -> winfsp::Result<()> {
        let new_path = path_from_winfsp(&wide_to_string(new_file_name));
        if !replace_if_exists {
            if self.shared.db.get_node_by_path(&new_path).ok().flatten().is_some() {
                return Err(FspError::NTSTATUS(STATUS_OBJECT_NAME_COLLISION.0));
            }
        }
        match self.rename_path(&context.path.lock(), &new_path) {
            Ok(()) => {
                *context.path.lock() = new_path.clone();
                if let Some(n) = context.node.lock().as_mut() {
                    let (_p, new_name) = crate::metadata::MetadataDb::parent_and_name(&new_path);
                    n.name = new_name;
                    n.virtual_path = new_path;
                }
                Ok(())
            }
            Err(e) => {
                tracing::warn!(error = %e, "rename failed");
                Err(FspError::NTSTATUS(STATUS_OBJECT_NAME_NOT_FOUND.0))
            }
        }
    }

    fn set_file_size(
        &self,
        context: &Self::FileContext,
        new_size: u64,
        _set_allocation_size: bool,
        file_info: &mut FileInfo,
    ) -> winfsp::Result<()> {
        let mut buf = context.write_buf.lock();
        if (new_size as usize) < buf.len() {
            buf.truncate(new_size as usize);
        } else {
            buf.resize(new_size as usize, 0);
        }
        *context.dirty.lock() = true;
        if let Some(node) = context.node.lock().as_mut() {
            node.size = new_size as i64;
            fill_info(node, file_info);
        }
        Ok(())
    }

    fn get_volume_info(&self, out_volume_info: &mut VolumeInfo) -> winfsp::Result<()> {
        let accounts = self.shared.db.list_accounts().unwrap_or_default();
        let connected: Vec<_> = accounts
            .into_iter()
            .filter(|a| a.status == "connected")
            .collect();
        let total: u64 = connected
            .iter()
            .filter_map(|a| a.quota_bytes.map(|q| q as u64))
            .sum();
        let used: u64 = connected
            .iter()
            .filter_map(|a| a.usage_bytes.map(|q| q as u64))
            .sum();
        out_volume_info.total_size = if total == 0 { 1 << 40 } else { total };
        out_volume_info.free_size = out_volume_info.total_size.saturating_sub(used);
        let label = std::ffi::OsString::from("Storbyte");
        out_volume_info.set_volume_label(&label);
        Ok(())
    }

    fn get_security(
        &self,
        _context: &Self::FileContext,
        security_descriptor: Option<&mut [c_void]>,
    ) -> winfsp::Result<u64> {
        let security_size = self.security.len() as u64;
        if let Some(buffer) = security_descriptor {
            if (buffer.len() as u64) >= security_size {
                unsafe {
                    std::ptr::copy_nonoverlapping(
                        self.security.as_ptr(),
                        buffer.as_mut_ptr() as *mut u8,
                        self.security.len(),
                    );
                }
            }
        }
        Ok(security_size)
    }
}

impl StorbyteFs {
    fn default_provider(&self) -> Option<Arc<dyn crate::providers::StorageProvider>> {
        let settings = self.shared.settings.read();
        let providers = self.shared.providers.read();
        if let Some(id) = &settings.default_account_id {
            if let Some(p) = providers.iter().find(|p| p.account_id() == id) {
                return Some(p.clone());
            }
        }
        providers.first().cloned()
    }

    fn create_path(&self, path: &str, is_directory: bool) -> anyhow::Result<NodeRow> {
        let (parent_path, name) = crate::metadata::MetadataDb::parent_and_name(path);
        let parent = self
            .shared
            .db
            .get_node_by_path(&parent_path)?
            .ok_or_else(|| anyhow::anyhow!("parent missing: {parent_path}"))?;
        let provider = self
            .shared
            .pick_upload_provider(&parent, &name, path, true)
            .ok_or_else(|| {
                if self.shared.providers.read().is_empty() {
                    anyhow::anyhow!("no connected account for writes")
                } else {
                    anyhow::anyhow!("Upload cancelled")
                }
            })?;
        let parent_remote = if parent.virtual_path == "\\" {
            "root".to_string()
        } else {
            // Parent folder must belong to the same account we upload into.
            if parent.account_id != provider.account_id() && parent.virtual_path != "\\" {
                return Err(anyhow::anyhow!(
                    "cannot create under folder owned by another account"
                ));
            }
            parent.remote_id.clone()
        };

        // Reuse an existing same-name folder (local or Drive) — never create a second
        // Drive folder that later syncs as "Name (XXXXXXXX)".
        if is_directory {
            if let Some(existing) = self.shared.db.find_child_by_name(
                parent.id,
                &name,
                true,
                Some(provider.account_id()),
            )? {
                return Ok(existing);
            }
        }

        let rt = self.shared.runtime.clone();
        let stat = if is_directory {
            let children = rt.block_on(provider.list_children(&parent_remote))?;
            if let Some(hit) = children
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
                rt.block_on(provider.create_directory(&parent_remote, &name))?
            }
        } else {
            rt.block_on(provider.create_file(&parent_remote, &name, None))?
        };
        let (id, actual_path) = self.shared.db.insert_node(
            Some(parent.id),
            &name,
            path,
            is_directory,
            stat.size as i64,
            stat.mime_type.as_deref(),
            &stat.id,
            provider.account_id(),
            stat.modified_time.as_deref(),
            None,
        )?;
        self.shared
            .db
            .get_node_by_path(&actual_path)?
            .ok_or_else(|| anyhow::anyhow!("created node missing locally (id={id})"))
    }

    fn flush_writes(&self, context: &FileCtx) -> anyhow::Result<()> {
        if !*context.dirty.lock() {
            return Ok(());
        }
        let data = context.write_buf.lock().clone();
        let node = context
            .node
            .lock()
            .clone()
            .ok_or_else(|| anyhow::anyhow!("no node for flush"))?;
        let provider = self
            .shared
            .provider_for(&node.account_id)
            .ok_or_else(|| anyhow::anyhow!("provider offline for {}", node.account_id))?;
        let rt = self.shared.runtime.clone();
        let stat = rt.block_on(provider.write_replace(&node.remote_id, &data))?;
        self.shared
            .db
            .update_node_remote(node.id, &stat.id, data.len() as i64, stat.modified_time.as_deref())?;
        if let Some(n) = context.node.lock().as_mut() {
            n.size = data.len() as i64;
            n.remote_id = stat.id;
        }
        *context.dirty.lock() = false;
        Ok(())
    }

    fn delete_path(&self, path: &str) -> anyhow::Result<()> {
        let node = self
            .shared
            .db
            .get_node_by_path(path)?
            .ok_or_else(|| anyhow::anyhow!("missing {path}"))?;

        let remote_id = node.remote_id.clone();
        let account_id = node.account_id.clone();
        let bytes = self
            .shared
            .db
            .sum_file_bytes_under_path(path)
            .map(|(_, b)| b)
            .unwrap_or_else(|_| node.size.max(0));

        // Local first — Explorer returns immediately (same as rename).
        self.shared.db.delete_node_by_path(path)?;
        self.shared.cache.invalidate_file(&remote_id);
        let _ = self.shared.db.adjust_account_usage(&account_id, -bytes);

        if let Some(provider) = self.shared.provider_for(&account_id) {
            self.shared.runtime.spawn(async move {
                if let Err(e) = provider.delete(&remote_id).await {
                    tracing::warn!(error = %e, remote_id = %remote_id, "background Drive delete failed");
                }
            });
        }
        Ok(())
    }

    fn rename_path(&self, old_path: &str, new_path: &str) -> anyhow::Result<()> {
        let node = self
            .shared
            .db
            .get_node_by_path(old_path)?
            .ok_or_else(|| anyhow::anyhow!("missing {old_path}"))?;
        let (_parent, new_name) = crate::metadata::MetadataDb::parent_and_name(new_path);

        // Local first so Explorer finishes instantly; Drive rename in background.
        self.shared
            .db
            .rename_node_tree(node.id, old_path, &new_name, new_path)?;

        if let Some(provider) = self.shared.provider_for(&node.account_id) {
            let remote_id = node.remote_id.clone();
            let new_name = new_name.clone();
            self.shared.runtime.spawn(async move {
                if let Err(e) = provider.rename(&remote_id, &new_name).await {
                    tracing::warn!(error = %e, remote_id = %remote_id, "background Drive rename failed");
                }
            });
        }
        Ok(())
    }
}

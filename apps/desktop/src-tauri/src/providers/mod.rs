mod google;

pub use google::GoogleDriveProvider;

use crate::scheduler::Priority;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RemoteEntry {
    pub id: String,
    pub name: String,
    pub is_directory: bool,
    pub size: u64,
    pub mime_type: Option<String>,
    pub modified_time: Option<String>,
    pub md5: Option<String>,
    pub parents: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RemoteStat {
    pub id: String,
    pub name: String,
    pub is_directory: bool,
    pub size: u64,
    pub mime_type: Option<String>,
    pub modified_time: Option<String>,
    pub md5: Option<String>,
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
pub struct QuotaInfo {
    pub limit_bytes: Option<u64>,
    pub usage_bytes: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RemoteChange {
    pub file_id: String,
    pub removed: bool,
    pub file: Option<RemoteEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChangesPage {
    pub changes: Vec<RemoteChange>,
    pub next_page_token: Option<String>,
    pub new_start_page_token: Option<String>,
}

#[async_trait]
pub trait StorageProvider: Send + Sync {
    fn provider_id(&self) -> &str;
    fn account_id(&self) -> &str;

    async fn list_children(&self, parent_remote_id: &str) -> anyhow::Result<Vec<RemoteEntry>>;

    /// Top-level shared drives (or equivalent). Default: none.
    async fn list_shared_roots(&self) -> anyhow::Result<Vec<RemoteEntry>> {
        Ok(Vec::new())
    }

    /// Flat metadata listing (all files/folders). Prefer this for fast indexing.
    /// Default: unsupported — callers should fall back to BFS via `list_children`.
    async fn list_all_metadata(&self) -> anyhow::Result<Vec<RemoteEntry>> {
        anyhow::bail!("flat metadata listing not supported for this provider")
    }

    /// Stream metadata pages (≤1000 rows each). Metadata only — never file bytes.
    /// Default chunks [`list_all_metadata`](Self::list_all_metadata).
    async fn list_metadata_pages(
        &self,
        mut on_page: Box<dyn FnMut(Vec<RemoteEntry>) -> anyhow::Result<()> + Send>,
    ) -> anyhow::Result<()> {
        let all = self.list_all_metadata().await?;
        for chunk in all.chunks(1000) {
            on_page(chunk.to_vec())?;
        }
        Ok(())
    }

    /// Cursor for Drive Changes API. Default: unsupported.
    async fn changes_start_page_token(&self) -> anyhow::Result<String> {
        anyhow::bail!("changes API not supported for this provider")
    }

    /// Incremental changes since `page_token`. Default: unsupported.
    async fn list_changes(&self, page_token: &str) -> anyhow::Result<ChangesPage> {
        let _ = page_token;
        anyhow::bail!("changes API not supported for this provider")
    }

    /// My Drive root folder id (not the alias "root"). Default: "root".
    async fn my_drive_root_id(&self) -> anyhow::Result<String> {
        Ok("root".into())
    }

    async fn stat(&self, remote_id: &str) -> anyhow::Result<RemoteStat>;
    async fn read_range(
        &self,
        remote_id: &str,
        offset: u64,
        length: u64,
        priority: Priority,
    ) -> anyhow::Result<Vec<u8>>;
    async fn about_quota(&self) -> anyhow::Result<Option<QuotaInfo>>;

    async fn create_file(
        &self,
        parent_remote_id: &str,
        name: &str,
        mime_type: Option<&str>,
    ) -> anyhow::Result<RemoteStat>;

    async fn create_directory(
        &self,
        parent_remote_id: &str,
        name: &str,
    ) -> anyhow::Result<RemoteStat>;

    async fn write_replace(&self, remote_id: &str, data: &[u8]) -> anyhow::Result<RemoteStat>;

    async fn rename(&self, remote_id: &str, new_name: &str) -> anyhow::Result<RemoteStat>;

    async fn delete(&self, remote_id: &str) -> anyhow::Result<()>;
}

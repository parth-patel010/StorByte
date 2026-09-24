use anyhow::{anyhow, Context, Result};
use async_trait::async_trait;
use serde::Deserialize;

use super::{RemoteEntry, RemoteStat, StorageProvider};
use crate::auth::ensure_access_token;
use crate::scheduler::{with_backoff, Priority, RequestScheduler};
use std::sync::Arc;

const FOLDER_MIME: &str = "application/vnd.google-apps.folder";

pub struct GoogleDriveProvider {
    account_id: String,
    client: reqwest::Client,
    scheduler: Arc<RequestScheduler>,
}

impl GoogleDriveProvider {
    pub fn new(account_id: String, scheduler: Arc<RequestScheduler>) -> Self {
        let client = reqwest::Client::builder()
            .pool_idle_timeout(std::time::Duration::from_secs(90))
            .pool_max_idle_per_host(8)
            .tcp_keepalive(std::time::Duration::from_secs(60))
            .build()
            .unwrap_or_else(|_| reqwest::Client::new());
        Self {
            account_id,
            client,
            scheduler,
        }
    }

    /// Paginated metadata-only `files.list` (pageSize=1000). Never fetches media.
    async fn list_metadata_corpus(
        &self,
        corpora: &str,
        drive_id: Option<&str>,
        out: &mut Vec<RemoteEntry>,
        seen: &mut std::collections::HashSet<String>,
    ) -> Result<()> {
        self.list_metadata_corpus_pages(corpora, drive_id, seen, &mut |page| {
            out.extend(page);
            Ok(())
        })
        .await
    }

    async fn list_metadata_corpus_pages(
        &self,
        corpora: &str,
        drive_id: Option<&str>,
        seen: &mut std::collections::HashSet<String>,
        on_page: &mut (dyn FnMut(Vec<RemoteEntry>) -> Result<()> + Send),
    ) -> Result<()> {
        let _permit = self.scheduler.acquire(Priority::Background).await;
        let token = ensure_access_token(&self.account_id).await?;
        let mut page_token: Option<String> = None;
        loop {
            let token = token.clone();
            let page = page_token.clone();
            let client = self.client.clone();
            let corpora = corpora.to_string();
            let drive_id = drive_id.map(|s| s.to_string());
            let list: DriveList = with_backoff(4, || {
                let token = token.clone();
                let page = page.clone();
                let client = client.clone();
                let corpora = corpora.clone();
                let drive_id = drive_id.clone();
                async move {
                    let mut req = client
                        .get("https://www.googleapis.com/drive/v3/files")
                        .bearer_auth(&token)
                        .query(&[
                            ("q", "trashed = false"),
                            (
                                "fields",
                                "nextPageToken,files(id,name,mimeType,size,modifiedTime,md5Checksum,parents)",
                            ),
                            ("pageSize", "1000"),
                            ("supportsAllDrives", "true"),
                            ("includeItemsFromAllDrives", "true"),
                            ("corpora", corpora.as_str()),
                        ]);
                    if let Some(id) = &drive_id {
                        req = req.query(&[("driveId", id.as_str())]);
                    } else {
                        req = req.query(&[("spaces", "drive")]);
                    }
                    if let Some(pt) = &page {
                        req = req.query(&[("pageToken", pt.as_str())]);
                    }
                    req.send()
                        .await?
                        .error_for_status()?
                        .json::<DriveList>()
                        .await
                        .map_err(|e| anyhow!(e))
                }
            })
            .await?;

            let mut page_entries =
                Vec::with_capacity(list.files.as_ref().map(|f| f.len()).unwrap_or(0));
            for f in list.files.unwrap_or_default() {
                let e = map_file(f);
                if seen.insert(e.id.clone()) {
                    page_entries.push(e);
                }
            }
            if !page_entries.is_empty() {
                on_page(page_entries)?;
            }
            page_token = list.next_page_token;
            if page_token.is_none() {
                break;
            }
        }
        Ok(())
    }
}

#[derive(Debug, Deserialize)]
struct DriveList {
    files: Option<Vec<DriveFile>>,
    #[serde(rename = "nextPageToken")]
    next_page_token: Option<String>,
}

#[derive(Debug, Deserialize)]
struct DriveFile {
    id: String,
    name: String,
    #[serde(rename = "mimeType")]
    mime_type: Option<String>,
    size: Option<String>,
    #[serde(rename = "modifiedTime")]
    modified_time: Option<String>,
    #[serde(rename = "md5Checksum")]
    md5: Option<String>,
    parents: Option<Vec<String>>,
}

#[derive(Debug, Deserialize)]
struct About {
    #[serde(rename = "storageQuota")]
    storage_quota: Option<StorageQuota>,
}

#[derive(Debug, Deserialize)]
struct StorageQuota {
    limit: Option<String>,
    usage: Option<String>,
}

fn map_file(f: DriveFile) -> RemoteEntry {
    let mime = f.mime_type.clone().unwrap_or_default();
    let is_directory = mime == FOLDER_MIME;
    let size = f
        .size
        .as_deref()
        .and_then(|s| s.parse::<u64>().ok())
        .unwrap_or(0);
    RemoteEntry {
        id: f.id,
        name: f.name,
        is_directory,
        size,
        mime_type: f.mime_type,
        modified_time: f.modified_time,
        md5: f.md5,
        parents: f.parents.unwrap_or_default(),
    }
}

#[async_trait]
impl StorageProvider for GoogleDriveProvider {
    fn provider_id(&self) -> &str {
        "google"
    }

    fn account_id(&self) -> &str {
        &self.account_id
    }

    async fn list_children(&self, parent_remote_id: &str) -> Result<Vec<RemoteEntry>> {
        let _permit = self.scheduler.acquire(Priority::Background).await;
        let token = ensure_access_token(&self.account_id).await?;
        let mut out = Vec::new();
        let mut page_token: Option<String> = None;
        let q = format!(
            "'{}' in parents and trashed = false",
            parent_remote_id.replace('\'', "\\'")
        );
        loop {
            let token = token.clone();
            let q = q.clone();
            let page = page_token.clone();
            let client = self.client.clone();
            let parent = parent_remote_id.to_string();
            let list: DriveList = with_backoff(4, || {
                let token = token.clone();
                let q = q.clone();
                let page = page.clone();
                let client = client.clone();
                let parent = parent.clone();
                async move {
                    let mut req = client
                        .get("https://www.googleapis.com/drive/v3/files")
                        .bearer_auth(&token)
                        .query(&[
                            ("q", q.as_str()),
                            (
                                "fields",
                                "nextPageToken,files(id,name,mimeType,size,modifiedTime,md5Checksum,parents)",
                            ),
                            ("pageSize", "1000"),
                            ("spaces", "drive"),
                            ("supportsAllDrives", "true"),
                            ("includeItemsFromAllDrives", "true"),
                        ]);
                    // My Drive root uses corpora=user; shared-drive folders need drive corpora.
                    if parent != "root" {
                        req = req.query(&[
                            ("corpora", "allDrives"),
                            ("supportsAllDrives", "true"),
                            ("includeItemsFromAllDrives", "true"),
                        ]);
                    } else {
                        req = req.query(&[("corpora", "user")]);
                    }
                    if let Some(pt) = &page {
                        req = req.query(&[("pageToken", pt.as_str())]);
                    }
                    req.send()
                        .await?
                        .error_for_status()?
                        .json::<DriveList>()
                        .await
                        .map_err(|e| anyhow!(e))
                }
            })
            .await
            .context("list children")?;
            for f in list.files.unwrap_or_default() {
                out.push(map_file(f));
            }
            page_token = list.next_page_token;
            if page_token.is_none() {
                break;
            }
        }
        Ok(out)
    }

    async fn list_shared_roots(&self) -> Result<Vec<RemoteEntry>> {
        let _permit = self.scheduler.acquire(Priority::Background).await;
        let token = ensure_access_token(&self.account_id).await?;
        let mut out = Vec::new();
        let mut page_token: Option<String> = None;
        loop {
            let token = token.clone();
            let page = page_token.clone();
            let client = self.client.clone();
            #[derive(Deserialize)]
            struct DrivesList {
                drives: Option<Vec<DriveRef>>,
                #[serde(rename = "nextPageToken")]
                next_page_token: Option<String>,
            }
            #[derive(Deserialize)]
            struct DriveRef {
                id: String,
                name: String,
            }
            let list: DrivesList = with_backoff(4, || {
                let token = token.clone();
                let page = page.clone();
                let client = client.clone();
                async move {
                    let mut req = client
                        .get("https://www.googleapis.com/drive/v3/drives")
                        .bearer_auth(&token)
                        .query(&[("pageSize", "100"), ("fields", "nextPageToken,drives(id,name)")]);
                    if let Some(pt) = &page {
                        req = req.query(&[("pageToken", pt.as_str())]);
                    }
                    req.send()
                        .await?
                        .error_for_status()?
                        .json::<DrivesList>()
                        .await
                        .map_err(|e| anyhow!(e))
                }
            })
            .await
            .context("list shared drives")?;
            for d in list.drives.unwrap_or_default() {
                out.push(RemoteEntry {
                    id: d.id,
                    name: d.name,
                    is_directory: true,
                    size: 0,
                    mime_type: Some(FOLDER_MIME.into()),
                    modified_time: None,
                    md5: None,
                    parents: vec!["root".into()],
                });
            }
            page_token = list.next_page_token;
            if page_token.is_none() {
                break;
            }
        }
        Ok(out)
    }

    async fn list_all_metadata(&self) -> Result<Vec<RemoteEntry>> {
        let mut out: Vec<RemoteEntry> = Vec::new();
        let mut seen = std::collections::HashSet::new();

        // My Drive (corpora=user) — fastest metadata-only flat list.
        self.list_metadata_corpus("user", None, &mut out, &mut seen)
            .await
            .context("flat list My Drive metadata")?;

        // Shared drives: one flat pass per drive (no per-folder BFS).
        if let Ok(shared) = self.list_shared_roots().await {
            for root in shared {
                let drive_id = root.id.clone();
                if seen.insert(root.id.clone()) {
                    out.push(root);
                }
                self.list_metadata_corpus("drive", Some(&drive_id), &mut out, &mut seen)
                    .await
                    .with_context(|| format!("flat list shared drive {drive_id}"))?;
            }
        }
        Ok(out)
    }

    async fn list_metadata_pages(
        &self,
        mut on_page: Box<dyn FnMut(Vec<RemoteEntry>) -> anyhow::Result<()> + Send>,
    ) -> Result<()> {
        let mut seen = std::collections::HashSet::new();
        self.list_metadata_corpus_pages("user", None, &mut seen, &mut on_page)
            .await
            .context("flat list My Drive metadata")?;

        if let Ok(shared) = self.list_shared_roots().await {
            for root in shared {
                let drive_id = root.id.clone();
                if seen.insert(root.id.clone()) {
                    on_page(vec![root])?;
                }
                self.list_metadata_corpus_pages("drive", Some(&drive_id), &mut seen, &mut on_page)
                    .await
                    .with_context(|| format!("flat list shared drive {drive_id}"))?;
            }
        }
        Ok(())
    }

    async fn stat(&self, remote_id: &str) -> Result<RemoteStat> {
        let _permit = self.scheduler.acquire(Priority::Background).await;
        let token = ensure_access_token(&self.account_id).await?;
        let f: DriveFile = self
            .client
            .get(format!("https://www.googleapis.com/drive/v3/files/{remote_id}"))
            .bearer_auth(&token)
            .query(&[
                (
                    "fields",
                    "id,name,mimeType,size,modifiedTime,md5Checksum,parents",
                ),
                ("supportsAllDrives", "true"),
            ])
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;
        let e = map_file(f);
        Ok(RemoteStat {
            id: e.id,
            name: e.name,
            is_directory: e.is_directory,
            size: e.size,
            mime_type: e.mime_type,
            modified_time: e.modified_time,
            md5: e.md5,
        })
    }

    async fn read_range(
        &self,
        remote_id: &str,
        offset: u64,
        length: u64,
        priority: Priority,
    ) -> Result<Vec<u8>> {
        if length == 0 {
            return Ok(Vec::new());
        }
        let _permit = self.scheduler.acquire(priority).await;
        let token = ensure_access_token(&self.account_id).await?;
        let end = offset + length - 1;
        let range = format!("bytes={offset}-{end}");
        let bytes = with_backoff(4, || async {
            let resp = self
                .client
                .get(format!("https://www.googleapis.com/drive/v3/files/{remote_id}"))
                .bearer_auth(&token)
                .query(&[("alt", "media"), ("supportsAllDrives", "true")])
                .header("Range", &range)
                .send()
                .await?
                .error_for_status()?;
            // Keep connection pooled; copy once into owned Vec for cache insertion.
            let data = resp.bytes().await?;
            Ok::<Vec<u8>, anyhow::Error>(data.to_vec())
        })
        .await
        .context("read_range")?;
        Ok(bytes)
    }

    async fn about_quota(&self) -> Result<Option<crate::providers::QuotaInfo>> {
        let token = ensure_access_token(&self.account_id).await?;
        let about: About = self
            .client
            .get("https://www.googleapis.com/drive/v3/about")
            .bearer_auth(&token)
            .query(&[("fields", "storageQuota")])
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;
        let Some(q) = about.storage_quota else {
            return Ok(None);
        };
        Ok(Some(crate::providers::QuotaInfo {
            limit_bytes: q.limit.and_then(|s| s.parse().ok()),
            usage_bytes: q.usage.and_then(|s| s.parse().ok()),
        }))
    }

    async fn create_file(
        &self,
        parent_remote_id: &str,
        name: &str,
        mime_type: Option<&str>,
    ) -> Result<RemoteStat> {
        let _permit = self.scheduler.acquire(Priority::UserRead).await;
        let token = ensure_access_token(&self.account_id).await?;
        let mime = mime_type.unwrap_or("application/octet-stream");
        let meta = serde_json::json!({
            "name": name,
            "parents": [parent_remote_id],
            "mimeType": mime,
        });
        let boundary = "storbyte_boundary";
        let body = format!(
            "--{boundary}\r\nContent-Type: application/json; charset=UTF-8\r\n\r\n{meta}\r\n--{boundary}\r\nContent-Type: {mime}\r\n\r\n\r\n--{boundary}--"
        );
        let f: DriveFile = self
            .client
            .post("https://www.googleapis.com/upload/drive/v3/files")
            .bearer_auth(&token)
            .query(&[
                ("uploadType", "multipart"),
                ("supportsAllDrives", "true"),
                (
                    "fields",
                    "id,name,mimeType,size,modifiedTime,md5Checksum,parents",
                ),
            ])
            .header(
                "Content-Type",
                format!("multipart/related; boundary={boundary}"),
            )
            .body(body)
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;
        let e = map_file(f);
        Ok(RemoteStat {
            id: e.id,
            name: e.name,
            is_directory: e.is_directory,
            size: e.size,
            mime_type: e.mime_type,
            modified_time: e.modified_time,
            md5: e.md5,
        })
    }

    async fn create_directory(&self, parent_remote_id: &str, name: &str) -> Result<RemoteStat> {
        let _permit = self.scheduler.acquire(Priority::UserRead).await;
        let token = ensure_access_token(&self.account_id).await?;
        let meta = serde_json::json!({
            "name": name,
            "parents": [parent_remote_id],
            "mimeType": FOLDER_MIME,
        });
        let f: DriveFile = self
            .client
            .post("https://www.googleapis.com/drive/v3/files")
            .bearer_auth(&token)
            .query(&[
                ("supportsAllDrives", "true"),
                (
                    "fields",
                    "id,name,mimeType,size,modifiedTime,md5Checksum,parents",
                ),
            ])
            .json(&meta)
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;
        let e = map_file(f);
        Ok(RemoteStat {
            id: e.id,
            name: e.name,
            is_directory: true,
            size: 0,
            mime_type: e.mime_type,
            modified_time: e.modified_time,
            md5: e.md5,
        })
    }

    async fn write_replace(&self, remote_id: &str, data: &[u8]) -> Result<RemoteStat> {
        let _permit = self.scheduler.acquire(Priority::UserRead).await;
        let token = ensure_access_token(&self.account_id).await?;
        let f: DriveFile = self
            .client
            .patch(format!(
                "https://www.googleapis.com/upload/drive/v3/files/{remote_id}"
            ))
            .bearer_auth(&token)
            .query(&[
                ("uploadType", "media"),
                ("supportsAllDrives", "true"),
                (
                    "fields",
                    "id,name,mimeType,size,modifiedTime,md5Checksum,parents",
                ),
            ])
            .header("Content-Type", "application/octet-stream")
            .body(data.to_vec())
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;
        let e = map_file(f);
        Ok(RemoteStat {
            id: e.id,
            name: e.name,
            is_directory: e.is_directory,
            size: e.size,
            mime_type: e.mime_type,
            modified_time: e.modified_time,
            md5: e.md5,
        })
    }

    async fn rename(&self, remote_id: &str, new_name: &str) -> Result<RemoteStat> {
        let _permit = self.scheduler.acquire(Priority::UserRead).await;
        let token = ensure_access_token(&self.account_id).await?;
        let f: DriveFile = self
            .client
            .patch(format!("https://www.googleapis.com/drive/v3/files/{remote_id}"))
            .bearer_auth(&token)
            .query(&[
                ("supportsAllDrives", "true"),
                (
                    "fields",
                    "id,name,mimeType,size,modifiedTime,md5Checksum,parents",
                ),
            ])
            .json(&serde_json::json!({ "name": new_name }))
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;
        let e = map_file(f);
        Ok(RemoteStat {
            id: e.id,
            name: e.name,
            is_directory: e.is_directory,
            size: e.size,
            mime_type: e.mime_type,
            modified_time: e.modified_time,
            md5: e.md5,
        })
    }

    async fn delete(&self, remote_id: &str) -> Result<()> {
        let _permit = self.scheduler.acquire(Priority::UserRead).await;
        let token = ensure_access_token(&self.account_id).await?;
        self.client
            .delete(format!("https://www.googleapis.com/drive/v3/files/{remote_id}"))
            .bearer_auth(&token)
            .query(&[("supportsAllDrives", "true")])
            .send()
            .await?
            .error_for_status()?;
        Ok(())
    }

    async fn changes_start_page_token(&self) -> Result<String> {
        let _permit = self.scheduler.acquire(Priority::Background).await;
        let token = ensure_access_token(&self.account_id).await?;
        #[derive(Deserialize)]
        struct StartToken {
            #[serde(rename = "startPageToken")]
            start_page_token: String,
        }
        let resp: StartToken = self
            .client
            .get("https://www.googleapis.com/drive/v3/changes/startPageToken")
            .bearer_auth(&token)
            .query(&[
                ("supportsAllDrives", "true"),
            ])
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;
        Ok(resp.start_page_token)
    }

    async fn list_changes(&self, page_token: &str) -> Result<super::ChangesPage> {
        let _permit = self.scheduler.acquire(Priority::Background).await;
        let token = ensure_access_token(&self.account_id).await?;
        #[derive(Deserialize)]
        struct ChangeFile {
            id: String,
            name: Option<String>,
            #[serde(rename = "mimeType")]
            mime_type: Option<String>,
            size: Option<String>,
            #[serde(rename = "modifiedTime")]
            modified_time: Option<String>,
            #[serde(rename = "md5Checksum")]
            md5: Option<String>,
            parents: Option<Vec<String>>,
            trashed: Option<bool>,
        }
        #[derive(Deserialize)]
        struct ChangeRow {
            #[serde(rename = "fileId")]
            file_id: Option<String>,
            removed: Option<bool>,
            file: Option<ChangeFile>,
        }
        #[derive(Deserialize)]
        struct ChangesList {
            changes: Option<Vec<ChangeRow>>,
            #[serde(rename = "nextPageToken")]
            next_page_token: Option<String>,
            #[serde(rename = "newStartPageToken")]
            new_start_page_token: Option<String>,
        }

        let list: ChangesList = with_backoff(4, || {
            let token = token.clone();
            let page_token = page_token.to_string();
            let client = self.client.clone();
            async move {
                client
                    .get("https://www.googleapis.com/drive/v3/changes")
                    .bearer_auth(&token)
                    .query(&[
                        ("pageToken", page_token.as_str()),
                        ("pageSize", "100"),
                        ("includeRemoved", "true"),
                        ("supportsAllDrives", "true"),
                        ("includeItemsFromAllDrives", "true"),
                        (
                            "fields",
                            "nextPageToken,newStartPageToken,changes(fileId,removed,file(id,name,mimeType,size,parents,trashed,modifiedTime,md5Checksum))",
                        ),
                    ])
                    .send()
                    .await?
                    .error_for_status()?
                    .json::<ChangesList>()
                    .await
                    .map_err(|e| anyhow!(e))
            }
        })
        .await
        .context("list changes")?;

        let mut changes = Vec::new();
        for row in list.changes.unwrap_or_default() {
            let file_id = row
                .file
                .as_ref()
                .map(|f| f.id.clone())
                .or(row.file_id)
                .unwrap_or_default();
            if file_id.is_empty() {
                continue;
            }
            let trashed = row.file.as_ref().and_then(|f| f.trashed).unwrap_or(false);
            let removed = row.removed.unwrap_or(false) || trashed;
            let file = if removed {
                None
            } else {
                row.file.map(|f| {
                    map_file(DriveFile {
                        id: f.id,
                        name: f.name.unwrap_or_else(|| "unnamed".into()),
                        mime_type: f.mime_type,
                        size: f.size,
                        modified_time: f.modified_time,
                        md5: f.md5,
                        parents: f.parents,
                    })
                })
            };
            changes.push(super::RemoteChange {
                file_id,
                removed,
                file,
            });
        }

        Ok(super::ChangesPage {
            changes,
            next_page_token: list.next_page_token,
            new_start_page_token: list.new_start_page_token,
        })
    }

    async fn my_drive_root_id(&self) -> Result<String> {
        let _permit = self.scheduler.acquire(Priority::Background).await;
        let token = ensure_access_token(&self.account_id).await?;
        #[derive(Deserialize)]
        struct RootFile {
            id: String,
        }
        let f: RootFile = self
            .client
            .get("https://www.googleapis.com/drive/v3/files/root")
            .bearer_auth(&token)
            .query(&[("fields", "id")])
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;
        Ok(f.id)
    }
}

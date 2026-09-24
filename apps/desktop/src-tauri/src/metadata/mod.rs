use anyhow::{Context, Result};
use chrono::Utc;
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::Instant;

use crate::config::data_dir;
use crate::providers::{RemoteEntry, StorageProvider};

/// Live metadata sync progress for the UI (percent + ETA).
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SyncProgress {
    pub active: bool,
    /// 0–100 overall progress estimate.
    pub percent: f64,
    pub items_indexed: u64,
    pub folders_done: u64,
    pub folders_pending: u64,
    pub account_index: u32,
    pub account_total: u32,
    pub account_label: String,
    /// Estimated seconds remaining, once enough samples exist.
    pub eta_secs: Option<u64>,
    pub message: String,
    /// Per-account status lines for the fetch UI.
    pub accounts: Vec<AccountSyncStatus>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AccountSyncStatus {
    pub label: String,
    /// waiting | indexing | complete | error
    pub status: String,
    pub items_indexed: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccountRow {
    pub id: String,
    pub email: String,
    pub provider: String,
    pub status: String,
    pub priority: i32,
    pub quota_bytes: Option<i64>,
    pub usage_bytes: Option<i64>,
    pub last_indexed_at: Option<String>,
    pub change_page_token: Option<String>,
    pub drive_root_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeRow {
    pub id: i64,
    pub parent_id: Option<i64>,
    pub name: String,
    pub virtual_path: String,
    pub is_directory: bool,
    pub size: i64,
    pub mime_type: Option<String>,
    pub remote_id: String,
    pub account_id: String,
    pub etag: Option<String>,
    pub modified_at: Option<String>,
    pub last_indexed: Option<String>,
}

pub struct MetadataDb {
    conn: Mutex<Connection>,
}

impl MetadataDb {
    pub fn open_default() -> Result<Self> {
        let path = data_dir().join("metadata.sqlite");
        Self::open(&path)
    }

    pub fn open(path: &Path) -> Result<Self> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let conn = Connection::open(path)?;
        let db = Self {
            conn: Mutex::new(conn),
        };
        db.migrate()?;
        Ok(db)
    }

    fn migrate(&self) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute_batch(
            r#"
            PRAGMA journal_mode=WAL;
            CREATE TABLE IF NOT EXISTS accounts (
              id TEXT PRIMARY KEY,
              email TEXT NOT NULL,
              provider TEXT NOT NULL,
              status TEXT NOT NULL,
              priority INTEGER NOT NULL DEFAULT 1,
              quota_bytes INTEGER,
              usage_bytes INTEGER,
              last_indexed_at TEXT
            );
            CREATE TABLE IF NOT EXISTS nodes (
              id INTEGER PRIMARY KEY AUTOINCREMENT,
              parent_id INTEGER,
              name TEXT NOT NULL,
              virtual_path TEXT NOT NULL UNIQUE,
              is_directory INTEGER NOT NULL,
              size INTEGER NOT NULL DEFAULT 0,
              mime_type TEXT,
              remote_id TEXT NOT NULL,
              account_id TEXT NOT NULL,
              etag TEXT,
              modified_at TEXT,
              last_indexed TEXT,
              children_synced_at TEXT,
              FOREIGN KEY(account_id) REFERENCES accounts(id)
            );
            CREATE INDEX IF NOT EXISTS idx_nodes_parent ON nodes(parent_id);
            CREATE INDEX IF NOT EXISTS idx_nodes_remote ON nodes(account_id, remote_id);
            CREATE INDEX IF NOT EXISTS idx_nodes_path ON nodes(virtual_path);
            "#,
        )?;
        // Additive migration for existing DBs.
        let _ = conn.execute("ALTER TABLE accounts ADD COLUMN usage_bytes INTEGER", []);
        let _ = conn.execute("ALTER TABLE nodes ADD COLUMN children_synced_at TEXT", []);
        let _ = conn.execute("ALTER TABLE accounts ADD COLUMN change_page_token TEXT", []);
        let _ = conn.execute("ALTER TABLE accounts ADD COLUMN drive_root_id TEXT", []);
        Ok(())
    }

    pub fn upsert_account(&self, account: &AccountRow) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            r#"
            INSERT INTO accounts (id, email, provider, status, priority, quota_bytes, usage_bytes, last_indexed_at)
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
            ON CONFLICT(id) DO UPDATE SET
              email=excluded.email,
              status=excluded.status,
              priority=excluded.priority,
              quota_bytes=excluded.quota_bytes,
              usage_bytes=excluded.usage_bytes,
              last_indexed_at=excluded.last_indexed_at
            "#,
            params![
                account.id,
                account.email,
                account.provider,
                account.status,
                account.priority,
                account.quota_bytes,
                account.usage_bytes,
                account.last_indexed_at
            ],
        )?;
        Ok(())
    }

    pub fn list_accounts(&self) -> Result<Vec<AccountRow>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, email, provider, status, priority, quota_bytes, usage_bytes, last_indexed_at, change_page_token, drive_root_id FROM accounts ORDER BY priority, email",
        )?;
        let rows = stmt
            .query_map([], |r| {
                Ok(AccountRow {
                    id: r.get(0)?,
                    email: r.get(1)?,
                    provider: r.get(2)?,
                    status: r.get(3)?,
                    priority: r.get(4)?,
                    quota_bytes: r.get(5)?,
                    usage_bytes: r.get(6)?,
                    last_indexed_at: r.get(7)?,
                    change_page_token: r.get(8).ok().flatten(),
                    drive_root_id: r.get(9).ok().flatten(),
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    pub fn delete_account(&self, account_id: &str) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute("DELETE FROM nodes WHERE account_id=?1", params![account_id])?;
        conn.execute("DELETE FROM accounts WHERE id=?1", params![account_id])?;
        Ok(())
    }

    pub fn set_account_status(&self, account_id: &str, status: &str) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "UPDATE accounts SET status=?1 WHERE id=?2",
            params![status, account_id],
        )?;
        Ok(())
    }

    pub fn get_account(&self, account_id: &str) -> Result<Option<AccountRow>> {
        Ok(self
            .list_accounts()?
            .into_iter()
            .find(|a| a.id == account_id))
    }

    pub fn update_node_size(&self, node_id: i64, size: i64) -> Result<()> {
        let now = Utc::now().to_rfc3339();
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "UPDATE nodes SET size=?1, modified_at=?2, last_indexed=?2 WHERE id=?3",
            params![size, now, node_id],
        )?;
        Ok(())
    }

    pub fn update_node_remote(
        &self,
        node_id: i64,
        remote_id: &str,
        size: i64,
        modified_at: Option<&str>,
    ) -> Result<()> {
        let now = Utc::now().to_rfc3339();
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "UPDATE nodes SET remote_id=?1, size=?2, modified_at=COALESCE(?3, ?4), last_indexed=?4 WHERE id=?5",
            params![remote_id, size, modified_at, now, node_id],
        )?;
        Ok(())
    }

    pub fn rename_node(&self, node_id: i64, new_name: &str, new_path: &str) -> Result<()> {
        let path = normalize_virtual_path(new_path);
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "UPDATE nodes SET name=?1, virtual_path=?2 WHERE id=?3",
            params![new_name, path, node_id],
        )?;
        Ok(())
    }

    /// Rename a node and rewrite descendant virtual_path prefixes in one shot.
    pub fn rename_node_tree(
        &self,
        node_id: i64,
        old_path: &str,
        new_name: &str,
        new_path: &str,
    ) -> Result<()> {
        let old = normalize_virtual_path(old_path);
        let newp = normalize_virtual_path(new_path);
        let safe_name = sanitize_name(new_name);
        let conn = self.conn.lock().unwrap();
        let tx = conn.unchecked_transaction()?;
        tx.execute(
            "UPDATE nodes SET name=?1, virtual_path=?2 WHERE id=?3",
            params![safe_name, newp, node_id],
        )?;
        // Children: `\Old\a` → `\New\a` via prefix replace.
        if old != "\\" {
            let like = format!("{old}\\%");
            // SQLite substr is 1-based; keep the suffix starting after `old`.
            let prefix_len = (old.len() + 1) as i64;
            tx.execute(
                "UPDATE nodes SET virtual_path = ?1 || substr(virtual_path, ?2) WHERE virtual_path LIKE ?3",
                params![newp, prefix_len, like],
            )?;
        }
        tx.commit()?;
        Ok(())
    }

    pub fn delete_node_by_path(&self, virtual_path: &str) -> Result<()> {
        let path = normalize_virtual_path(virtual_path);
        let conn = self.conn.lock().unwrap();
        // Delete node and descendants by path prefix
        conn.execute(
            "DELETE FROM nodes WHERE virtual_path=?1 OR virtual_path LIKE ?2",
            params![path, format!("{path}\\%")],
        )?;
        Ok(())
    }

    /// Sum file sizes under a path (for optimistic quota adjust on local delete).
    pub fn sum_file_bytes_under_path(&self, virtual_path: &str) -> Result<(String, i64)> {
        let path = normalize_virtual_path(virtual_path);
        let conn = self.conn.lock().unwrap();
        let account_id: String = conn.query_row(
            "SELECT account_id FROM nodes WHERE virtual_path=?1",
            params![path],
            |r| r.get(0),
        )?;
        let bytes: i64 = conn.query_row(
            "SELECT COALESCE(SUM(size), 0) FROM nodes \
             WHERE account_id=?1 AND is_directory=0 \
               AND (virtual_path=?2 OR virtual_path LIKE ?3)",
            params![account_id, path, format!("{path}\\%")],
            |r| r.get(0),
        )?;
        Ok((account_id, bytes.max(0)))
    }

    /// Optimistically nudge cached Drive usage after local create/delete.
    pub fn adjust_account_usage(&self, account_id: &str, delta: i64) -> Result<()> {
        if delta == 0 {
            return Ok(());
        }
        let mut accounts = self.list_accounts()?;
        let Some(a) = accounts.iter_mut().find(|a| a.id == account_id) else {
            return Ok(());
        };
        let cur = a.usage_bytes.unwrap_or(0);
        a.usage_bytes = Some((cur + delta).max(0));
        self.upsert_account(a)?;
        Ok(())
    }

    pub fn parent_and_name(path: &str) -> (String, String) {
        let p = normalize_virtual_path(path);
        if p == "\\" {
            return ("\\".into(), String::new());
        }
        match p.rsplit_once('\\') {
            Some(("", name)) => ("\\".into(), name.to_string()),
            Some((parent, name)) if parent.is_empty() => ("\\".into(), name.to_string()),
            Some((parent, name)) => (normalize_virtual_path(parent), name.to_string()),
            None => ("\\".into(), p),
        }
    }

    pub fn get_node_by_path(&self, virtual_path: &str) -> Result<Option<NodeRow>> {
        let path = normalize_virtual_path(virtual_path);
        let conn = self.conn.lock().unwrap();
        conn.query_row(
            r#"SELECT id, parent_id, name, virtual_path, is_directory, size, mime_type,
                      remote_id, account_id, etag, modified_at, last_indexed
               FROM nodes WHERE virtual_path=?1"#,
            params![path],
            |r| {
                Ok(NodeRow {
                    id: r.get(0)?,
                    parent_id: r.get(1)?,
                    name: r.get(2)?,
                    virtual_path: r.get(3)?,
                    is_directory: r.get::<_, i64>(4)? != 0,
                    size: r.get(5)?,
                    mime_type: r.get(6)?,
                    remote_id: r.get(7)?,
                    account_id: r.get(8)?,
                    etag: r.get(9)?,
                    modified_at: r.get(10)?,
                    last_indexed: r.get(11)?,
                })
            },
        )
        .optional()
        .map_err(Into::into)
    }

    pub fn search_nodes(&self, query: &str, limit: i64) -> Result<Vec<NodeRow>> {
        let q = query.trim();
        if q.is_empty() {
            return Ok(Vec::new());
        }
        let like = format!("%{}%", q.replace('%', "").replace('_', ""));
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            r#"SELECT id, parent_id, name, virtual_path, is_directory, size, mime_type,
                      remote_id, account_id, etag, modified_at, last_indexed
               FROM nodes
               WHERE virtual_path != char(92)
                 AND name LIKE ?1 COLLATE NOCASE
               ORDER BY is_directory DESC, name COLLATE NOCASE
               LIMIT ?2"#,
        )?;
        let rows = stmt
            .query_map(params![like, limit], map_node)?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    pub fn list_children(&self, parent_path: &str) -> Result<Vec<NodeRow>> {
        let parent = self.get_node_by_path(parent_path)?;
        let conn = self.conn.lock().unwrap();
        let sql = r#"SELECT id, parent_id, name, virtual_path, is_directory, size, mime_type,
                            remote_id, account_id, etag, modified_at, last_indexed
                     FROM nodes WHERE parent_id=?1
                     ORDER BY is_directory DESC, name COLLATE NOCASE"#;
        if let Some(p) = parent {
            let mut stmt = conn.prepare(sql)?;
            let rows = stmt
                .query_map(params![p.id], map_node)?
                .collect::<Result<Vec<_>, _>>()?;
            // Root node itself has parent_id NULL; its children have parent_id = root id.
            // Exclude the root row if someone queries incorrectly.
            Ok(rows
                .into_iter()
                .filter(|n| n.virtual_path != "\\")
                .collect())
        } else if normalize_virtual_path(parent_path) == "\\" {
            let mut stmt = conn.prepare(
                r#"SELECT id, parent_id, name, virtual_path, is_directory, size, mime_type,
                          remote_id, account_id, etag, modified_at, last_indexed
                   FROM nodes WHERE parent_id IS NULL AND virtual_path != '\\'
                   ORDER BY is_directory DESC, name COLLATE NOCASE"#,
            )?;
            let rows = stmt
                .query_map([], map_node)?
                .collect::<Result<Vec<_>, _>>()?;
            Ok(rows)
        } else {
            Ok(Vec::new())
        }
    }

    /// Find a direct child by display name (case-insensitive), optionally scoped to an account.
    pub fn find_child_by_name(
        &self,
        parent_id: i64,
        name: &str,
        is_directory: bool,
        account_id: Option<&str>,
    ) -> Result<Option<NodeRow>> {
        let safe = sanitize_name(name);
        let conn = self.conn.lock().unwrap();
        let dir = if is_directory { 1i64 } else { 0i64 };
        if let Some(aid) = account_id {
            let mut stmt = conn.prepare(
                r#"SELECT id, parent_id, name, virtual_path, is_directory, size, mime_type,
                          remote_id, account_id, etag, modified_at, last_indexed
                   FROM nodes
                   WHERE parent_id=?1 AND is_directory=?2 AND account_id=?3
                     AND name=?4 COLLATE NOCASE
                   LIMIT 1"#,
            )?;
            Ok(stmt
                .query_row(params![parent_id, dir, aid, safe], map_node)
                .optional()?)
        } else {
            let mut stmt = conn.prepare(
                r#"SELECT id, parent_id, name, virtual_path, is_directory, size, mime_type,
                          remote_id, account_id, etag, modified_at, last_indexed
                   FROM nodes
                   WHERE parent_id=?1 AND is_directory=?2 AND name=?3 COLLATE NOCASE
                   LIMIT 1"#,
            )?;
            Ok(stmt
                .query_row(params![parent_id, dir, safe], map_node)
                .optional()?)
        }
    }

    pub fn ensure_root(&self, account_id: &str) -> Result<i64> {
        if let Some(n) = self.get_node_by_path("\\")? {
            return Ok(n.id);
        }
        let now = Utc::now().to_rfc3339();
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO nodes (parent_id, name, virtual_path, is_directory, size, mime_type, remote_id, account_id, last_indexed)
             VALUES (NULL, '', char(92), 1, 0, 'inode/directory', 'root', ?1, ?2)",
            params![account_id, now],
        )?;
        Ok(conn.last_insert_rowid())
    }

    pub fn clear_account_nodes(&self, account_id: &str) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        // Use char(92) ('\') so we never mis-escape the root path in SQL string literals.
        conn.execute(
            "DELETE FROM nodes WHERE account_id=?1 AND virtual_path != char(92)",
            params![account_id],
        )?;
        Ok(())
    }

    /// Full wipe of indexed nodes (keeps root). Used before a clean sync.
    pub fn clear_all_nodes_except_root(&self) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute("DELETE FROM nodes WHERE virtual_path != char(92)", [])?;
        Ok(())
    }

    pub fn count_nodes(&self) -> Result<i64> {
        let conn = self.conn.lock().unwrap();
        let n: i64 = conn.query_row("SELECT COUNT(*) FROM nodes", [], |r| r.get(0))?;
        Ok(n)
    }

    /// True when at least one account finished a metadata sync.
    pub fn any_account_indexed(&self) -> Result<bool> {
        let conn = self.conn.lock().unwrap();
        let n: i64 = conn.query_row(
            "SELECT COUNT(*) FROM accounts WHERE last_indexed_at IS NOT NULL AND status != 'disconnected'",
            [],
            |r| r.get(0),
        )?;
        Ok(n > 0)
    }

    pub fn insert_node(
        &self,
        parent_id: Option<i64>,
        name: &str,
        virtual_path: &str,
        is_directory: bool,
        size: i64,
        mime_type: Option<&str>,
        remote_id: &str,
        account_id: &str,
        modified_at: Option<&str>,
        etag: Option<&str>,
    ) -> Result<(i64, String)> {
        let conn = self.conn.lock().unwrap();
        insert_node_on_conn(
            &conn,
            parent_id,
            name,
            virtual_path,
            is_directory,
            size,
            mime_type,
            remote_id,
            account_id,
            modified_at,
            etag,
        )
    }

    /// Insert many metadata rows in a single SQLite transaction (no file contents).
    pub fn insert_nodes_batch(&self, batch: &[PendingNode]) -> Result<Vec<(i64, String)>> {
        let mut conn = self.conn.lock().unwrap();
        let tx = conn.transaction()?;
        let mut out = Vec::with_capacity(batch.len());
        for item in batch {
            let row = insert_node_on_conn(
                &tx,
                item.parent_id,
                &item.name,
                &item.virtual_path,
                item.is_directory,
                item.size,
                item.mime_type.as_deref(),
                &item.remote_id,
                &item.account_id,
                item.modified_at.as_deref(),
                item.etag.as_deref(),
            )?;
            out.push(row);
        }
        tx.commit()?;
        Ok(out)
    }

    /// Fast metadata batch insert for fresh sync (no per-row path probing).
    pub fn insert_nodes_batch_fast(&self, batch: &[PendingNode]) -> Result<Vec<(i64, String)>> {
        let mut conn = self.conn.lock().unwrap();
        let tx = conn.transaction()?;
        let now = Utc::now().to_rfc3339();
        let mut out = Vec::with_capacity(batch.len());
        {
            let mut stmt = tx.prepare_cached(
                r#"INSERT INTO nodes (parent_id, name, virtual_path, is_directory, size, mime_type, remote_id, account_id, etag, modified_at, last_indexed)
                   VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)"#,
            )?;
            for item in batch {
                let safe_name = sanitize_name(&item.name);
                let vpath = normalize_virtual_path(&item.virtual_path);
                stmt.execute(params![
                    item.parent_id,
                    safe_name,
                    vpath,
                    if item.is_directory { 1 } else { 0 },
                    item.size,
                    item.mime_type,
                    &item.remote_id,
                    &item.account_id,
                    item.etag,
                    item.modified_at,
                    now,
                ])?;
                out.push((tx.last_insert_rowid(), vpath));
            }
        }
        tx.commit()?;
        Ok(out)
    }

    pub fn mark_indexed(&self, account_id: &str) -> Result<()> {
        let now = Utc::now().to_rfc3339();
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "UPDATE accounts SET last_indexed_at=?1, status='connected' WHERE id=?2",
            params![now, account_id],
        )?;
        Ok(())
    }

    pub fn touch_node_indexed(&self, node_id: i64) -> Result<()> {
        let now = Utc::now().to_rfc3339();
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "UPDATE nodes SET last_indexed=?1, children_synced_at=?1 WHERE id=?2",
            params![now, node_id],
        )?;
        Ok(())
    }

    pub fn get_change_page_token(&self, account_id: &str) -> Result<Option<String>> {
        let conn = self.conn.lock().unwrap();
        let v: Option<String> = conn
            .query_row(
                "SELECT change_page_token FROM accounts WHERE id=?1",
                params![account_id],
                |r| r.get(0),
            )
            .optional()?
            .flatten();
        Ok(v)
    }

    pub fn set_change_page_token(&self, account_id: &str, token: &str) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "UPDATE accounts SET change_page_token=?1 WHERE id=?2",
            params![token, account_id],
        )?;
        Ok(())
    }

    pub fn get_drive_root_id(&self, account_id: &str) -> Result<Option<String>> {
        let conn = self.conn.lock().unwrap();
        Ok(conn
            .query_row(
                "SELECT drive_root_id FROM accounts WHERE id=?1",
                params![account_id],
                |r| r.get(0),
            )
            .optional()?
            .flatten())
    }

    pub fn set_drive_root_id(&self, account_id: &str, root_id: &str) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "UPDATE accounts SET drive_root_id=?1 WHERE id=?2",
            params![root_id, account_id],
        )?;
        Ok(())
    }

    pub fn get_node_by_remote(
        &self,
        account_id: &str,
        remote_id: &str,
    ) -> Result<Option<NodeRow>> {
        let conn = self.conn.lock().unwrap();
        conn.query_row(
            r#"SELECT id, parent_id, name, virtual_path, is_directory, size, mime_type,
                      remote_id, account_id, etag, modified_at, last_indexed
               FROM nodes WHERE account_id=?1 AND remote_id=?2"#,
            params![account_id, remote_id],
            map_node,
        )
        .optional()
        .map_err(Into::into)
    }

    pub fn delete_by_remote(&self, account_id: &str, remote_id: &str) -> Result<usize> {
        let Some(node) = self.get_node_by_remote(account_id, remote_id)? else {
            return Ok(0);
        };
        let conn = self.conn.lock().unwrap();
        let like = format!("{}\\%", node.virtual_path);
        let n = conn.execute(
            "DELETE FROM nodes WHERE account_id=?1 AND (virtual_path = ?2 OR virtual_path LIKE ?3)",
            params![account_id, node.virtual_path, like],
        )?;
        Ok(n)
    }

    pub fn clear_children_synced(&self, node_id: i64) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "UPDATE nodes SET children_synced_at=NULL WHERE id=?1",
            params![node_id],
        )?;
        Ok(())
    }

    /// True when folder children should be refreshed from Drive (missing stamp or older than `max_age_secs`).
    pub fn folder_children_stale(&self, virtual_path: &str, max_age_secs: i64) -> Result<bool> {
        let path = normalize_virtual_path(virtual_path);
        let conn = self.conn.lock().unwrap();
        let row: Option<(Option<String>, bool)> = conn
            .query_row(
                "SELECT children_synced_at, is_directory FROM nodes WHERE virtual_path=?1",
                params![path],
                |r| Ok((r.get(0)?, r.get::<_, i64>(1)? != 0)),
            )
            .optional()?;
        let Some((synced_at, is_dir)) = row else {
            return Ok(path == "\\");
        };
        if !is_dir {
            return Ok(false);
        }
        let Some(ts_str) = synced_at else {
            return Ok(true);
        };
        let Ok(ts) = chrono::DateTime::parse_from_rfc3339(&ts_str) else {
            return Ok(true);
        };
        let age = (Utc::now() - ts.with_timezone(&Utc)).num_seconds();
        Ok(age > max_age_secs)
    }

    /// Remove direct children of `parent_id` for `account_id` whose remote_id is not in `keep`.
    pub fn delete_direct_children_except(
        &self,
        parent_id: i64,
        account_id: &str,
        keep_remote_ids: &std::collections::HashSet<String>,
    ) -> Result<usize> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            r#"SELECT virtual_path, remote_id FROM nodes
               WHERE parent_id=?1 AND account_id=?2"#,
        )?;
        let rows: Vec<(String, String)> = stmt
            .query_map(params![parent_id, account_id], |r| {
                Ok((r.get(0)?, r.get(1)?))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        drop(stmt);

        let mut deleted = 0usize;
        for (vpath, remote_id) in rows {
            if keep_remote_ids.contains(&remote_id) {
                continue;
            }
            let like = format!("{vpath}\\%");
            deleted += conn.execute(
                "DELETE FROM nodes WHERE virtual_path = ?1 OR virtual_path LIKE ?2",
                params![vpath, like],
            )?;
        }
        Ok(deleted)
    }

    /// Upsert Drive children under a parent folder (or root). Returns items upserted.
    pub fn upsert_remote_children(
        &self,
        parent_id: i64,
        parent_path: &str,
        account_id: &str,
        children: &[RemoteEntry],
    ) -> Result<u64> {
        let parent_path = normalize_virtual_path(parent_path);
        let mut keep = std::collections::HashSet::new();
        let mut count = 0u64;
        for child in children {
            keep.insert(child.id.clone());
            let vpath = join_path(&parent_path, &sanitize_name(&child.name));
            self.insert_node(
                Some(parent_id),
                &child.name,
                &vpath,
                child.is_directory,
                child.size as i64,
                child.mime_type.as_deref(),
                &child.id,
                account_id,
                child.modified_time.as_deref(),
                child.md5.as_deref(),
            )?;
            count += 1;
        }
        self.delete_direct_children_except(parent_id, account_id, &keep)?;
        self.touch_node_indexed(parent_id)?;
        Ok(count)
    }
}

fn map_node(r: &rusqlite::Row<'_>) -> rusqlite::Result<NodeRow> {
    Ok(NodeRow {
        id: r.get(0)?,
        parent_id: r.get(1)?,
        name: r.get(2)?,
        virtual_path: r.get(3)?,
        is_directory: r.get::<_, i64>(4)? != 0,
        size: r.get(5)?,
        mime_type: r.get(6)?,
        remote_id: r.get(7)?,
        account_id: r.get(8)?,
        etag: r.get(9)?,
        modified_at: r.get(10)?,
        last_indexed: r.get(11)?,
    })
}

/// One metadata row ready for a batch SQLite insert (no file bytes).
#[derive(Debug, Clone)]
pub struct PendingNode {
    pub parent_id: Option<i64>,
    pub name: String,
    pub virtual_path: String,
    pub is_directory: bool,
    pub size: i64,
    pub mime_type: Option<String>,
    pub remote_id: String,
    pub account_id: String,
    pub modified_at: Option<String>,
    pub etag: Option<String>,
}

fn insert_node_on_conn(
    conn: &Connection,
    parent_id: Option<i64>,
    name: &str,
    virtual_path: &str,
    is_directory: bool,
    size: i64,
    mime_type: Option<&str>,
    remote_id: &str,
    account_id: &str,
    modified_at: Option<&str>,
    etag: Option<&str>,
) -> Result<(i64, String)> {
    let now = Utc::now().to_rfc3339();
    let safe_name = sanitize_name(name);
    let desired = {
        let (parent, _) = MetadataDb::parent_and_name(&normalize_virtual_path(virtual_path));
        join_path(&parent, &safe_name)
    };
    let parent = parent_path_of(&desired);

    if let Some((existing_id, existing_path)) = conn
        .query_row(
            "SELECT id, virtual_path FROM nodes WHERE account_id=?1 AND remote_id=?2",
            params![account_id, remote_id],
            |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?)),
        )
        .optional()?
    {
        let final_path = if existing_path == desired {
            existing_path
        } else {
            // Prefer the desired path when free; otherwise keep the existing path.
            let taken: bool = conn
                .query_row(
                    "SELECT 1 FROM nodes WHERE virtual_path=?1 AND id!=?2",
                    params![desired, existing_id],
                    |_| Ok(true),
                )
                .optional()?
                .unwrap_or(false);
            if taken {
                existing_path
            } else {
                desired.clone()
            }
        };
        let final_name = final_path
            .rsplit('\\')
            .next()
            .unwrap_or(safe_name.as_str())
            .to_string();
        conn.execute(
            r#"UPDATE nodes SET parent_id=?1, name=?2, virtual_path=?3, is_directory=?4, size=?5, mime_type=?6,
               etag=?7, modified_at=?8, last_indexed=?9 WHERE id=?10"#,
            params![
                parent_id,
                final_name,
                final_path,
                if is_directory { 1 } else { 0 },
                size,
                mime_type,
                etag,
                modified_at,
                now,
                existing_id
            ],
        )?;
        return Ok((existing_id, final_path));
    }

    for attempt in 0..50 {
        let final_path = allocate_unique_path(conn, &parent, &safe_name, remote_id, attempt)?;
        let final_name = final_path
            .rsplit('\\')
            .next()
            .unwrap_or(safe_name.as_str())
            .to_string();
        let final_path = normalize_virtual_path(&final_path);
        match conn.execute(
            r#"INSERT INTO nodes (parent_id, name, virtual_path, is_directory, size, mime_type, remote_id, account_id, etag, modified_at, last_indexed)
               VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)"#,
            params![
                parent_id,
                final_name,
                final_path,
                if is_directory { 1 } else { 0 },
                size,
                mime_type,
                remote_id,
                account_id,
                etag,
                modified_at,
                now
            ],
        ) {
            Ok(_) => return Ok((conn.last_insert_rowid(), final_path)),
            Err(e) => {
                let msg = e.to_string();
                if msg.contains("UNIQUE") || msg.contains("unique") {
                    continue;
                }
                return Err(e.into());
            }
        }
    }
    anyhow::bail!("could not insert node after retries: {safe_name}")
}

pub fn normalize_virtual_path(path: &str) -> String {
    let mut p = path.replace('/', "\\");
    if p.is_empty() {
        return "\\".into();
    }
    if !p.starts_with('\\') {
        p = format!("\\{p}");
    }
    while p.contains("\\\\") {
        p = p.replace("\\\\", "\\");
    }
    if p.len() > 1 && p.ends_with('\\') {
        p.pop();
    }
    p
}

fn parent_path_of(path: &str) -> String {
    let p = normalize_virtual_path(path);
    if p == "\\" {
        return "\\".into();
    }
    match p.rsplit_once('\\') {
        Some(("", _)) => "\\".into(),
        Some((parent, _)) if parent.is_empty() => "\\".into(),
        Some((parent, _)) => normalize_virtual_path(parent),
        None => "\\".into(),
    }
}

fn join_path(parent: &str, name: &str) -> String {
    let parent = normalize_virtual_path(parent);
    if parent == "\\" {
        format!("\\{name}")
    } else {
        format!("{parent}\\{name}")
    }
}

fn split_name(name: &str) -> (String, String) {
    if let Some((stem, ext)) = name.rsplit_once('.') {
        if !stem.is_empty() {
            return (stem.to_string(), format!(".{ext}"));
        }
    }
    (name.to_string(), String::new())
}

fn sanitize_name(name: &str) -> String {
    let cleaned: String = name
        .chars()
        .map(|c| match c {
            '\\' | '/' | ':' | '*' | '?' | '"' | '<' | '>' | '|' | '\0' => '_',
            c if c.is_control() => '_',
            c => c,
        })
        .collect::<String>()
        .trim()
        .trim_matches('.')
        .to_string();
    if cleaned.is_empty() {
        "unnamed".into()
    } else {
        cleaned
    }
}

fn path_exists(conn: &Connection, path: &str) -> Result<bool> {
    Ok(conn
        .query_row(
            "SELECT 1 FROM nodes WHERE virtual_path=?1 COLLATE NOCASE",
            params![path],
            |_| Ok(true),
        )
        .optional()?
        .unwrap_or(false))
}

fn allocate_unique_path(
    conn: &Connection,
    parent: &str,
    name: &str,
    remote_id: &str,
    attempt: usize,
) -> Result<String> {
    let (stem, ext) = split_name(name);
    let short = if remote_id.len() > 8 {
        &remote_id[remote_id.len() - 8..]
    } else {
        remote_id
    };

    let candidates: Vec<String> = if attempt == 0 {
        vec![
            join_path(parent, name),
            join_path(parent, &format!("{stem} ({short}){ext}")),
        ]
    } else {
        vec![join_path(
            parent,
            &format!("{stem} ({short}-{attempt}){ext}"),
        )]
    };

    for candidate in candidates {
        if !path_exists(conn, &candidate)? {
            return Ok(candidate);
        }
    }
    // Last resort: uuid fragment
    let uniq = uuid::Uuid::new_v4().simple().to_string();
    Ok(join_path(
        parent,
        &format!("{stem} ({short}-{}){ext}", &uniq[..8]),
    ))
}

fn build_progress(
    account_index: u32,
    account_total: u32,
    account_label: &str,
    folders_done: u64,
    folders_pending: u64,
    items_indexed: u64,
    started: Instant,
) -> SyncProgress {
    let account_total = account_total.max(1);
    let folder_total = folders_done + folders_pending;
    let folder_frac = if folder_total == 0 {
        0.0
    } else {
        folders_done as f64 / folder_total as f64
    };
    // Blend per-account folder progress into overall % across accounts.
    let percent = ((account_index as f64 + folder_frac) / account_total as f64 * 100.0)
        .clamp(0.0, 99.5);

    let elapsed = started.elapsed().as_secs_f64();
    let eta_secs = if folders_done >= 2 && elapsed > 0.5 && folders_pending > 0 {
        let rate = folders_done as f64 / elapsed;
        if rate > 0.0 {
            Some((folders_pending as f64 / rate).ceil() as u64)
        } else {
            None
        }
    } else {
        None
    };

    let message = if folders_pending == 0 && folders_done > 0 {
        format!("Finishing {account_label}…")
    } else {
        format!("Indexing {account_label}")
    };

    SyncProgress {
        active: true,
        percent,
        items_indexed,
        folders_done,
        folders_pending,
        account_index,
        account_total,
        account_label: account_label.to_string(),
        eta_secs,
        message,
        accounts: vec![],
    }
}

/// Breadth-first index of My Drive (+ shared drives) into SQLite.
/// Prefer [`sync_account`] which tries flat listing first.
pub async fn sync_account_bfs(
    db: &MetadataDb,
    provider: &dyn StorageProvider,
    account_index: u32,
    account_total: u32,
    account_label: &str,
    mut on_progress: impl FnMut(SyncProgress) + Send,
) -> Result<u64> {
    let account_id = provider.account_id().to_string();
    let started = Instant::now();
    db.ensure_root(&account_id)?;
    // Wipe this account's tree so re-sync never fights leftover UNIQUE paths.
    db.clear_account_nodes(&account_id)?;
    let root_id = db.ensure_root(&account_id)?;

    let mut queue: Vec<(String, i64, String)> = vec![("root".into(), root_id, "\\".into())];
    let mut seen_remote: std::collections::HashSet<String> = std::collections::HashSet::new();
    let mut folders_done: u64 = 0;
    let mut items_indexed: u64 = 0;
    let mut last_emit = Instant::now() - std::time::Duration::from_secs(1);

    // Seed shared drives as top-level folders under V:\
    if let Ok(shared) = provider.list_shared_roots().await {
        for child in shared {
            if !seen_remote.insert(child.id.clone()) {
                continue;
            }
            let vpath = join_path("\\", &sanitize_name(&child.name));
            let (id, actual_path) = db.insert_node(
                Some(root_id),
                &child.name,
                &vpath,
                true,
                0,
                child.mime_type.as_deref(),
                &child.id,
                &account_id,
                None,
                None,
            )?;
            items_indexed += 1;
            queue.push((child.id, id, actual_path));
        }
    }

    on_progress(build_progress(
        account_index,
        account_total,
        account_label,
        0,
        queue.len().max(1) as u64,
        items_indexed,
        started,
    ));

    while let Some((remote_parent, parent_row_id, parent_path)) = queue.pop() {
        let children = provider
            .list_children(&remote_parent)
            .await
            .with_context(|| format!("list {remote_parent}"))?;
        for child in children {
            if !seen_remote.insert(child.id.clone()) {
                continue; // skip duplicate listings of the same Drive file id
            }
            let vpath = join_path(&parent_path, &sanitize_name(&child.name));
            let (id, actual_path) = db.insert_node(
                Some(parent_row_id),
                &child.name,
                &vpath,
                child.is_directory,
                child.size as i64,
                child.mime_type.as_deref(),
                &child.id,
                &account_id,
                child.modified_time.as_deref(),
                child.md5.as_deref(),
            )?;
            items_indexed += 1;
            if child.is_directory {
                queue.push((child.id, id, actual_path));
            }
        }
        folders_done += 1;

        // Throttle UI updates (~4/sec) while always emitting on completion ticks.
        if last_emit.elapsed().as_millis() >= 250 || queue.is_empty() {
            last_emit = Instant::now();
            on_progress(build_progress(
                account_index,
                account_total,
                account_label,
                folders_done,
                queue.len() as u64,
                items_indexed,
                started,
            ));
        }
    }

    if let Ok(Some(q)) = provider.about_quota().await {
        let mut accounts = db.list_accounts()?;
        if let Some(a) = accounts.iter_mut().find(|a| a.id == account_id) {
            a.quota_bytes = q.limit_bytes.map(|v| v as i64);
            a.usage_bytes = q.usage_bytes.map(|v| v as i64);
            db.upsert_account(a)?;
        }
    }
    db.mark_indexed(&account_id)?;
    seed_change_cursor(db, provider).await;
    Ok(items_indexed)
}

fn sync_account_flat_build(
    db: &MetadataDb,
    account_id: &str,
    account_index: u32,
    account_total: u32,
    account_label: &str,
    root_id: i64,
    by_id: std::collections::HashMap<String, RemoteEntry>,
    _children: std::collections::HashMap<String, Vec<String>>,
    started: Instant,
    on_progress: &mut impl FnMut(SyncProgress),
) -> Result<u64> {
    let total_hint = by_id.len().max(1) as u64;

    // Resolve virtual paths + depth (parents first) — streaming index needs name/size only.
    let mut path_memo: std::collections::HashMap<String, String> =
        std::collections::HashMap::with_capacity(by_id.len());
    let mut depth_memo: std::collections::HashMap<String, u32> =
        std::collections::HashMap::with_capacity(by_id.len());

    fn resolve_path_depth(
        id: &str,
        by_id: &std::collections::HashMap<String, RemoteEntry>,
        path_memo: &mut std::collections::HashMap<String, String>,
        depth_memo: &mut std::collections::HashMap<String, u32>,
        stack: &mut std::collections::HashSet<String>,
    ) -> (String, u32) {
        if let Some(d) = depth_memo.get(id) {
            return (path_memo.get(id).cloned().unwrap_or_else(|| "\\".into()), *d);
        }
        if !stack.insert(id.to_string()) {
            return ("\\unnamed".into(), 0);
        }
        let Some(entry) = by_id.get(id) else {
            stack.remove(id);
            return ("\\unnamed".into(), 0);
        };
        let (parent_path, depth) = entry
            .parents
            .iter()
            .find(|p| by_id.contains_key(*p))
            .map(|p| {
                let (pp, pd) = resolve_path_depth(p, by_id, path_memo, depth_memo, stack);
                (pp, pd + 1)
            })
            .unwrap_or_else(|| ("\\".into(), 0));
        let path = join_path(&parent_path, &sanitize_name(&entry.name));
        path_memo.insert(id.to_string(), path.clone());
        depth_memo.insert(id.to_string(), depth);
        stack.remove(id);
        (path, depth)
    }

    let mut ordered: Vec<(String, String, u32)> = Vec::with_capacity(by_id.len());
    let mut stack = std::collections::HashSet::new();
    for id in by_id.keys() {
        let (path, depth) = resolve_path_depth(id, &by_id, &mut path_memo, &mut depth_memo, &mut stack);
        ordered.push((id.clone(), path, depth));
    }
    ordered.sort_by_key(|(_, _, d)| *d);

    let mut remote_to_local: std::collections::HashMap<String, (i64, String)> =
        std::collections::HashMap::new();
    remote_to_local.insert("root".into(), (root_id, "\\".into()));

    let mut batch: Vec<PendingNode> = Vec::with_capacity(1000);
    let mut items_indexed: u64 = 0;
    let mut last_emit = Instant::now() - std::time::Duration::from_secs(1);

    for (remote_id, vpath, _) in ordered {
        let Some(entry) = by_id.get(&remote_id) else {
            continue;
        };
        let parent_row_id = entry
            .parents
            .iter()
            .find_map(|p| remote_to_local.get(p).map(|(id, _)| *id))
            .unwrap_or(root_id);

        batch.push(PendingNode {
            parent_id: Some(parent_row_id),
            name: entry.name.clone(),
            virtual_path: vpath,
            is_directory: entry.is_directory,
            size: entry.size as i64,
            mime_type: entry.mime_type.clone(),
            remote_id: entry.id.clone(),
            account_id: account_id.to_string(),
            modified_at: entry.modified_time.clone(),
            etag: entry.md5.clone(),
        });

        if batch.len() >= 1000 {
            let remotes: Vec<String> = batch.iter().map(|b| b.remote_id.clone()).collect();
            let results = db.insert_nodes_batch_fast(&batch)?;
            items_indexed += results.len() as u64;
            for (remote, (local_id, path)) in remotes.into_iter().zip(results.into_iter()) {
                remote_to_local.insert(remote, (local_id, path));
            }
            batch.clear();

            if last_emit.elapsed().as_millis() >= 200 {
                last_emit = Instant::now();
                let frac = (items_indexed as f64 / total_hint as f64).min(1.0);
                let percent = ((account_index as f64 + frac) / account_total.max(1) as f64 * 100.0)
                    .clamp(0.0, 99.5);
                on_progress(SyncProgress {
                    active: true,
                    percent,
                    items_indexed,
                    folders_done: items_indexed,
                    folders_pending: total_hint.saturating_sub(items_indexed),
                    account_index,
                    account_total,
                    account_label: account_label.to_string(),
                    eta_secs: eta_from(started, items_indexed, total_hint),
                    message: "Fetching your data from drive".into(),
                    accounts: vec![],
                });
            }
        }
    }

    if !batch.is_empty() {
        let remotes: Vec<String> = batch.iter().map(|b| b.remote_id.clone()).collect();
        let results = db.insert_nodes_batch_fast(&batch)?;
        items_indexed += results.len() as u64;
        for (remote, (local_id, path)) in remotes.into_iter().zip(results.into_iter()) {
            remote_to_local.insert(remote, (local_id, path));
        }
    }

    let _ = remote_to_local;
    let _ = started;
    Ok(items_indexed)
}


fn eta_from(started: Instant, done: u64, total: u64) -> Option<u64> {
    if done < 50 || total == 0 || done >= total {
        return None;
    }
    let elapsed = started.elapsed().as_secs_f64();
    let rate = done as f64 / elapsed.max(0.001);
    if rate <= 0.0 {
        return None;
    }
    Some(((total - done) as f64 / rate).ceil() as u64)
}

/// Index only My Drive root + shared-drive roots (fast intro / shallow sync).
pub async fn sync_account_root_only(
    db: &MetadataDb,
    provider: &dyn StorageProvider,
    account_index: u32,
    account_total: u32,
    account_label: &str,
    mut on_progress: impl FnMut(SyncProgress) + Send,
) -> Result<u64> {
    let account_id = provider.account_id().to_string();
    let started = Instant::now();
    db.ensure_root(&account_id)?;
    db.clear_account_nodes(&account_id)?;
    let root_id = db.ensure_root(&account_id)?;

    on_progress(build_progress(
        account_index,
        account_total,
        account_label,
        0,
        1,
        0,
        started,
    ));

    let mut entries: Vec<RemoteEntry> = Vec::new();
    if let Ok(shared) = provider.list_shared_roots().await {
        entries.extend(shared);
    }
    let children = provider
        .list_children("root")
        .await
        .with_context(|| format!("list root for {account_id}"))?;
    entries.extend(children);

    let items = db.upsert_remote_children(root_id, "\\", &account_id, &entries)?;

    if let Ok(Some(q)) = provider.about_quota().await {
        let mut accounts = db.list_accounts()?;
        if let Some(a) = accounts.iter_mut().find(|a| a.id == account_id) {
            a.quota_bytes = q.limit_bytes.map(|v| v as i64);
            a.usage_bytes = q.usage_bytes.map(|v| v as i64);
            db.upsert_account(a)?;
        }
    }
    db.mark_indexed(&account_id)?;
    // Reset change cursor so we only watch future cloud edits.
    seed_change_cursor(db, provider).await;
    on_progress(build_progress(
        account_index,
        account_total,
        account_label,
        1,
        0,
        items,
        started,
    ));
    Ok(items)
}

async fn seed_change_cursor(db: &MetadataDb, provider: &dyn StorageProvider) {
    let account_id = provider.account_id();
    if let Ok(token) = provider.changes_start_page_token().await {
        let _ = db.set_change_page_token(account_id, &token);
    }
    if let Ok(root_id) = provider.my_drive_root_id().await {
        let _ = db.set_drive_root_id(account_id, &root_id);
    }
}

/// Fetch one folder's children from Drive and upsert into SQLite.
pub async fn ensure_folder_children(
    db: &MetadataDb,
    provider: &dyn StorageProvider,
    virtual_path: &str,
) -> Result<u64> {
    let path = normalize_virtual_path(virtual_path);
    let account_id = provider.account_id().to_string();

    if path == "\\" {
        let root_id = db.ensure_root(&account_id)?;
        let mut entries: Vec<RemoteEntry> = Vec::new();
        if let Ok(shared) = provider.list_shared_roots().await {
            entries.extend(shared);
        }
        let children = provider.list_children("root").await?;
        entries.extend(children);
        return db.upsert_remote_children(root_id, "\\", &account_id, &entries);
    }

    let node = db
        .get_node_by_path(&path)?
        .ok_or_else(|| anyhow::anyhow!("folder not found: {path}"))?;
    if !node.is_directory {
        return Ok(0);
    }
    if node.account_id != account_id {
        return Ok(0);
    }
    let children = provider
        .list_children(&node.remote_id)
        .await
        .with_context(|| format!("list children of {}", node.remote_id))?;
    db.upsert_remote_children(node.id, &node.virtual_path, &account_id, &children)
}

/// Index one account: flat list first, BFS fallback. Metadata only.
pub async fn sync_account(
    db: &MetadataDb,
    provider: &dyn StorageProvider,
    account_index: u32,
    account_total: u32,
    account_label: &str,
    mut on_progress: impl FnMut(SyncProgress) + Send,
) -> Result<u64> {
    match sync_account_flat_full(
        db,
        provider,
        account_index,
        account_total,
        account_label,
        &mut on_progress,
    )
    .await
    {
        Ok(n) => Ok(n),
        Err(e) => {
            tracing::warn!("flat metadata sync failed, falling back to BFS: {e:#}");
            sync_account_bfs(
                db,
                provider,
                account_index,
                account_total,
                account_label,
                on_progress,
            )
            .await
        }
    }
}

async fn sync_account_flat_full(
    db: &MetadataDb,
    provider: &dyn StorageProvider,
    account_index: u32,
    account_total: u32,
    account_label: &str,
    on_progress: &mut impl FnMut(SyncProgress),
) -> Result<u64> {
    let account_id = provider.account_id().to_string();
    let started = Instant::now();
    db.ensure_root(&account_id)?;
    db.clear_account_nodes(&account_id)?;
    let root_id = db.ensure_root(&account_id)?;

    on_progress(SyncProgress {
        active: true,
        percent: (((account_index as f64 + 0.05) / account_total.max(1) as f64) * 100.0)
            .clamp(1.0, 99.5),
        message: "Fetching your data from drive".into(),
        account_index,
        account_total,
        account_label: account_label.to_string(),
        ..Default::default()
    });

    // Page pipeline: merge each API page into local parent→child maps, then batch insert.
    let by_id = std::sync::Arc::new(std::sync::Mutex::new(
        std::collections::HashMap::<String, RemoteEntry>::new(),
    ));
    let children = std::sync::Arc::new(std::sync::Mutex::new(
        std::collections::HashMap::<String, Vec<String>>::new(),
    ));
    let listed = std::sync::Arc::new(std::sync::atomic::AtomicU64::new(0));
    let pages = std::sync::Arc::new(std::sync::atomic::AtomicU64::new(0));
    let listing_done = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));

    let by_id_c = by_id.clone();
    let children_c = children.clone();
    let listed_c = listed.clone();
    let pages_c = pages.clone();

    let listed_poll = listed.clone();
    let pages_poll = pages.clone();
    let listing_poll = listing_done.clone();
    let account_index_poll = account_index;
    let account_total_poll = account_total;
    let account_label_poll = account_label.to_string();

    let poll = async {
        let mut last_emit = Instant::now() - std::time::Duration::from_millis(500);
        while !listing_poll.load(std::sync::atomic::Ordering::Relaxed) {
            if last_emit.elapsed() >= std::time::Duration::from_millis(250) {
                let p = pages_poll.load(std::sync::atomic::Ordering::Relaxed);
                let l = listed_poll.load(std::sync::atomic::Ordering::Relaxed);
                let frac = if p == 0 {
                    0.08
                } else {
                    (0.08 + (p as f64 * 0.04).min(0.55)).min(0.65)
                };
                on_progress(SyncProgress {
                    active: true,
                    percent: (((account_index_poll as f64 + frac)
                        / account_total_poll.max(1) as f64)
                        * 100.0)
                        .clamp(1.0, 99.5),
                    items_indexed: l,
                    folders_done: p,
                    account_index: account_index_poll,
                    account_total: account_total_poll,
                    account_label: account_label_poll.clone(),
                    message: "Fetching your data from drive".into(),
                    ..Default::default()
                });
                last_emit = Instant::now();
            }
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        }
    };

    let list = async {
        let result = provider
            .list_metadata_pages(Box::new(move |page| {
                let n = page.len() as u64;
                listed_c.fetch_add(n, std::sync::atomic::Ordering::Relaxed);
                pages_c.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                let mut by_id = by_id_c.lock().unwrap();
                let mut children = children_c.lock().unwrap();
                for e in page {
                    if e.parents.is_empty() {
                        children
                            .entry("root".into())
                            .or_default()
                            .push(e.id.clone());
                    } else {
                        for p in &e.parents {
                            children.entry(p.clone()).or_default().push(e.id.clone());
                        }
                    }
                    by_id.insert(e.id.clone(), e);
                }
                Ok(())
            }))
            .await;
        listing_done.store(true, std::sync::atomic::Ordering::Relaxed);
        result
    };

    let (list_result, _) = tokio::join!(list, poll);
    list_result?;

    let listed_n = listed.load(std::sync::atomic::Ordering::Relaxed);
    let pages_n = pages.load(std::sync::atomic::Ordering::Relaxed);
    on_progress(SyncProgress {
        active: true,
        percent: ((account_index as f64 + 0.4) / account_total.max(1) as f64 * 100.0)
            .clamp(0.0, 99.5),
        items_indexed: listed_n,
        folders_done: pages_n,
        account_index,
        account_total,
        account_label: account_label.to_string(),
        message: "Fetching your data from drive".into(),
        ..Default::default()
    });

    let by_id = std::mem::take(&mut *by_id.lock().unwrap());
    let children = std::mem::take(&mut *children.lock().unwrap());

    let n = sync_account_flat_build(
        db,
        &account_id,
        account_index,
        account_total,
        account_label,
        root_id,
        by_id,
        children,
        started,
        on_progress,
    )?;

    db.mark_indexed(&account_id)?;
    seed_change_cursor(db, provider).await;
    Ok(n)
}

/// Apply Drive Changes API rows into SQLite. Returns number of applied mutations.
pub async fn apply_drive_changes(
    db: &MetadataDb,
    provider: &dyn StorageProvider,
) -> Result<u64> {
    let account_id = provider.account_id().to_string();
    let mut token = match db.get_change_page_token(&account_id)? {
        Some(t) => t,
        None => {
            seed_change_cursor(db, provider).await;
            return Ok(0);
        }
    };

    if db.get_drive_root_id(&account_id)?.is_none() {
        if let Ok(root_id) = provider.my_drive_root_id().await {
            let _ = db.set_drive_root_id(&account_id, &root_id);
        }
    }
    let drive_root = db.get_drive_root_id(&account_id)?.unwrap_or_else(|| "root".into());
    let root_id = db.ensure_root(&account_id)?;

    let mut applied = 0u64;
    loop {
        let page = provider.list_changes(&token).await?;
        for change in &page.changes {
            if change.removed {
                applied += db.delete_by_remote(&account_id, &change.file_id)? as u64;
                continue;
            }
            let Some(file) = &change.file else {
                continue;
            };
            // Resolve first known parent (or My Drive root).
            let mut parent_row: Option<(i64, String)> = None;
            for pid in &file.parents {
                if pid == "root" || pid == &drive_root {
                    parent_row = Some((root_id, "\\".into()));
                    break;
                }
                if let Some(n) = db.get_node_by_remote(&account_id, pid)? {
                    parent_row = Some((n.id, n.virtual_path.clone()));
                    break;
                }
            }
            let Some((parent_id, parent_path)) = parent_row else {
                // Parent not indexed yet (lazy tree) — skip; opening that folder will fetch it.
                continue;
            };
            let vpath = join_path(&parent_path, &sanitize_name(&file.name));
            db.insert_node(
                Some(parent_id),
                &file.name,
                &vpath,
                file.is_directory,
                file.size as i64,
                file.mime_type.as_deref(),
                &file.id,
                &account_id,
                file.modified_time.as_deref(),
                file.md5.as_deref(),
            )?;
            applied += 1;
        }

        if let Some(next) = page.next_page_token {
            token = next;
            continue;
        }
        if let Some(new_start) = page.new_start_page_token {
            db.set_change_page_token(&account_id, &new_start)?;
        }
        break;
    }

    Ok(applied)
}

pub fn db_path() -> PathBuf {
    data_dir().join("metadata.sqlite")
}

#[allow(dead_code)]
fn _use_remote_entry(_: RemoteEntry) {}

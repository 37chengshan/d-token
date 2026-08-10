//! SQLite `PersistencePort` 实现 — store 只保留 persistence（九边界架构边界，D5）。
//!
//! `dtoken-context-bus` 定义 [`dtoken_context_bus::persistence::PersistencePort`]；
//! 本模块以 SQLite（WAL，私有权限，与 `ContextStore` 同一安全基线）实现该 port，
//! 承载 bus 的 session cache 条目 / Recovery manifest / SharedContext grant 持久化。
//!
//! 安全边界（与 bus 契约一致）：
//! - 不持久化 raw/secret/prompt/source：条目 body 只存在于进程内
//!   session cache / relay（memory-only），本模块**永不落盘 body 列**——
//!   表中根本没有 body 列，恢复出的条目恒为元数据投影（body=None）；
//! - 按 project 隔离读取（跨项目污染 = 0）；
//! - delete 语义：TTL/stale/superseded 清理后条目/ manifest 从表中删除，重启不复活。
//!
//! 表结构（`bus_entry` / `bus_manifest` / `bus_grant`）与 `ContextStore` 的
//! 业务表完全分离；本模块不触碰 memory/CCR/capability 投影。

use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::Duration;

use anyhow::{Context, Result};
use dtoken_context_bus::cache::CacheEntry;
use dtoken_context_bus::persistence::PersistencePort;
use dtoken_context_bus::recovery::{ManifestEntry, RecoveryManifest};
use dtoken_context_bus::shared::SharedGrant;
use rusqlite::{params, Connection};

/// SQLite 持久化 port（生产默认；WAL + 私有文件权限）。
#[derive(Debug)]
pub struct SqlitePersistence {
    connection: Mutex<Connection>,
    path: PathBuf,
}

impl SqlitePersistence {
    /// 打开（或创建）bus 持久化数据库。路径父目录必须存在或可创建；
    /// 文件权限按私有存储基线收紧（unix 0600 / windows DACL）。
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        crate::ensure_private_storage_platform_support()?;
        let path = path.as_ref();
        if let Ok(metadata) = std::fs::symlink_metadata(path) {
            if metadata.file_type().is_symlink() || !metadata.is_file() {
                anyhow::bail!("bus persistence path must be a regular file");
            }
        }
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).with_context(|| {
                format!("create bus persistence directory {}", parent.display())
            })?;
            crate::restrict_store_directory_permissions(parent)?;
        }
        let connection = Connection::open(path)
            .with_context(|| format!("open bus persistence {}", path.display()))?;
        connection.busy_timeout(Duration::from_millis(500))?;
        connection.execute_batch(
            "PRAGMA foreign_keys = ON;
             PRAGMA journal_mode = WAL;
             PRAGMA synchronous = NORMAL;
             CREATE TABLE IF NOT EXISTS bus_entry (
                 map_signature TEXT PRIMARY KEY,
                 project_id TEXT NOT NULL,
                 agent_id TEXT NOT NULL,
                 session_id TEXT NOT NULL,
                 source_fingerprint TEXT NOT NULL,
                 recorded_at_ms INTEGER NOT NULL,
                 ttl_seconds INTEGER,
                 body_bytes INTEGER NOT NULL,
                 query_tokens TEXT NOT NULL
             );
             CREATE TABLE IF NOT EXISTS bus_manifest (
                 manifest_id TEXT PRIMARY KEY,
                 project_id TEXT NOT NULL,
                 request_id TEXT NOT NULL DEFAULT '',
                 source_digest TEXT NOT NULL DEFAULT '',
                 decision_digest TEXT NOT NULL DEFAULT '',
                 created_at_ms INTEGER NOT NULL,
                 entries_json TEXT NOT NULL
             );
             CREATE TABLE IF NOT EXISTS bus_grant (
                 grant_id TEXT PRIMARY KEY,
                 project_id TEXT NOT NULL,
                 owner_agent_id TEXT NOT NULL,
                 grantee_agent_id TEXT NOT NULL,
                 session_id TEXT NOT NULL,
                 grant_revision TEXT NOT NULL,
                 granted_at_ms INTEGER NOT NULL,
                 revoked_at_ms INTEGER
             );
             CREATE INDEX IF NOT EXISTS bus_entry_project_idx ON bus_entry(project_id);
             CREATE INDEX IF NOT EXISTS bus_manifest_project_idx ON bus_manifest(project_id);
             CREATE INDEX IF NOT EXISTS bus_grant_project_idx ON bus_grant(project_id);",
        )?;
        // 旧 schema（body TEXT 列）迁移：检测 + 重建 + WAL/VACUUM 擦除。
        migrate_legacy_body_column(&connection)?;
        crate::restrict_store_permissions(path)?;
        crate::restrict_store_sidecar_permissions(path)?;
        Ok(Self {
            connection: Mutex::new(connection),
            path: path.to_path_buf(),
        })
    }

    /// 数据库文件路径（诊断/审计）。
    pub fn path(&self) -> &Path {
        &self.path
    }
}

fn tokens_from_row(value: String) -> Result<Vec<String>> {
    serde_json::from_str(&value)
        .map_err(|_| anyhow::anyhow!("query_tokens must be a JSON string array"))
}

/// 旧 schema 迁移：首个 C2/C3 集成 commit 创建的 `bus_entry` 带
/// `body TEXT` 列，可能落盘明文 prompt/source。`CREATE TABLE IF NOT EXISTS`
/// 无法改变既有表，因此打开时检测 body 列并重建为无 body 的新表
/// （旧明文行不复制），随后 checkpoint+TRUNCATE WAL、secure_delete+VACUUM
/// 擦除释放页中的明文。全新数据库（无 body 列）为 no-op。
fn migrate_legacy_body_column(connection: &Connection) -> Result<()> {
    let has_body = {
        let mut statement = connection.prepare("PRAGMA table_info(bus_entry)")?;
        let mut rows = statement.query([])?;
        let mut found = false;
        while let Some(row) = rows.next()? {
            let name: String = row.get(1)?;
            if name == "body" {
                found = true;
            }
        }
        found
    };
    if !has_body {
        return Ok(());
    }
    connection
        .execute_batch(
            "BEGIN IMMEDIATE;
             CREATE TABLE bus_entry_migrated (
                 map_signature TEXT PRIMARY KEY,
                 project_id TEXT NOT NULL,
                 agent_id TEXT NOT NULL,
                 session_id TEXT NOT NULL,
                 source_fingerprint TEXT NOT NULL,
                 recorded_at_ms INTEGER NOT NULL,
                 ttl_seconds INTEGER,
                 body_bytes INTEGER NOT NULL,
                 query_tokens TEXT NOT NULL
             );
             INSERT INTO bus_entry_migrated (
                 map_signature, project_id, agent_id, session_id, source_fingerprint,
                 recorded_at_ms, ttl_seconds, body_bytes, query_tokens
             ) SELECT map_signature, project_id, agent_id, session_id, source_fingerprint,
                 recorded_at_ms, ttl_seconds, body_bytes, query_tokens
               FROM bus_entry;
             DROP TABLE bus_entry;
             ALTER TABLE bus_entry_migrated RENAME TO bus_entry;
             CREATE INDEX IF NOT EXISTS bus_entry_project_idx ON bus_entry(project_id);
             COMMIT;",
        )
        .context("rebuild legacy bus_entry without body column")?;
    // 擦除：先 checkpoint+TRUNCATE WAL（可能持有旧明文帧），再以
    // secure_delete 重写主库（释放页清零），最后截断 VACUUM 产生的 WAL。
    connection
        .execute_batch("PRAGMA wal_checkpoint(TRUNCATE);")
        .context("truncate WAL after legacy migration")?;
    connection
        .execute_batch("PRAGMA secure_delete = ON; VACUUM;")
        .context("vacuum legacy database with secure_delete")?;
    connection
        .execute_batch("PRAGMA wal_checkpoint(TRUNCATE);")
        .context("truncate post-vacuum WAL")?;
    Ok(())
}

impl PersistencePort for SqlitePersistence {
    fn save_entry(&mut self, entry: &CacheEntry) -> Result<()> {
        // 隐私边界（SECURITY/PRD）：持久层绝不落盘 prompt/response/source 原文。
        // 仅存元数据与签名；body 只存在于进程内 session cache / relay
        // （memory-only），重启后恢复的条目 body 恒为 None。
        let tokens = serde_json::to_string(&entry.query_tokens)?;
        let connection = self
            .connection
            .lock()
            .map_err(|_| anyhow::anyhow!("bus persistence lock poisoned"))?;
        connection.execute(
            "INSERT INTO bus_entry (
                 map_signature, project_id, agent_id, session_id, source_fingerprint,
                 recorded_at_ms, ttl_seconds, body_bytes, query_tokens
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
             ON CONFLICT(map_signature) DO UPDATE SET
                 project_id = excluded.project_id,
                 agent_id = excluded.agent_id,
                 session_id = excluded.session_id,
                 source_fingerprint = excluded.source_fingerprint,
                 recorded_at_ms = excluded.recorded_at_ms,
                 ttl_seconds = excluded.ttl_seconds,
                 body_bytes = excluded.body_bytes,
                 query_tokens = excluded.query_tokens",
            params![
                entry.map_signature,
                entry.project_id,
                entry.agent_id,
                entry.session_id,
                entry.source_fingerprint,
                entry.recorded_at_ms as i64,
                entry.ttl_seconds.map(|ttl| ttl as i64),
                entry.body_bytes as i64,
                tokens,
            ],
        )?;
        Ok(())
    }

    fn load_entries(&self, project_id: &str) -> Result<Vec<CacheEntry>> {
        let connection = self
            .connection
            .lock()
            .map_err(|_| anyhow::anyhow!("bus persistence lock poisoned"))?;
        let mut statement = connection.prepare(
            "SELECT map_signature, project_id, agent_id, session_id, source_fingerprint,
                    recorded_at_ms, ttl_seconds, body_bytes, query_tokens
             FROM bus_entry WHERE project_id = ?1 ORDER BY recorded_at_ms",
        )?;
        let rows = statement.query_map(params![project_id], |row| {
            let tokens = tokens_from_row(row.get::<_, String>(8)?).map_err(|error| {
                rusqlite::Error::FromSqlConversionFailure(
                    8,
                    rusqlite::types::Type::Text,
                    error.into(),
                )
            })?;
            Ok(CacheEntry {
                map_signature: row.get(0)?,
                project_id: row.get(1)?,
                agent_id: row.get(2)?,
                session_id: row.get(3)?,
                source_fingerprint: row.get(4)?,
                recorded_at_ms: row.get::<_, i64>(5)? as u64,
                ttl_seconds: row.get::<_, Option<i64>>(6)?.map(|ttl| ttl as u64),
                body_bytes: row.get::<_, i64>(7)? as usize,
                query_tokens: tokens,
                // 持久层不存 body：恢复出的条目恒为元数据投影。
                body: None,
            })
        })?;
        let mut entries = Vec::new();
        for row in rows {
            entries.push(row?);
        }
        Ok(entries)
    }

    fn delete_entry(&mut self, project_id: &str, map_signature: &str) -> Result<()> {
        let connection = self
            .connection
            .lock()
            .map_err(|_| anyhow::anyhow!("bus persistence lock poisoned"))?;
        connection.execute(
            "DELETE FROM bus_entry WHERE project_id = ?1 AND map_signature = ?2",
            params![project_id, map_signature],
        )?;
        Ok(())
    }

    fn save_manifest(&mut self, manifest: &RecoveryManifest) -> Result<()> {
        let entries_json = serde_json::to_string(&manifest.entries)?;
        let connection = self
            .connection
            .lock()
            .map_err(|_| anyhow::anyhow!("bus persistence lock poisoned"))?;
        connection.execute(
            "INSERT INTO bus_manifest (
                 manifest_id, project_id, request_id, source_digest, decision_digest,
                 created_at_ms, entries_json
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
             ON CONFLICT(manifest_id) DO UPDATE SET
                 project_id = excluded.project_id,
                 request_id = excluded.request_id,
                 source_digest = excluded.source_digest,
                 decision_digest = excluded.decision_digest,
                 created_at_ms = excluded.created_at_ms,
                 entries_json = excluded.entries_json",
            params![
                manifest.manifest_id,
                manifest.project_id,
                manifest.request_id,
                manifest.source_digest,
                manifest.decision_digest,
                manifest.created_at_ms as i64,
                entries_json,
            ],
        )?;
        Ok(())
    }

    fn load_manifests(&self, project_id: &str) -> Result<Vec<RecoveryManifest>> {
        let connection = self
            .connection
            .lock()
            .map_err(|_| anyhow::anyhow!("bus persistence lock poisoned"))?;
        let mut statement = connection.prepare(
            "SELECT manifest_id, project_id, request_id, source_digest, decision_digest,
                    created_at_ms, entries_json
             FROM bus_manifest WHERE project_id = ?1 ORDER BY created_at_ms",
        )?;
        let rows = statement.query_map(params![project_id], |row| {
            let entries: Vec<ManifestEntry> =
                serde_json::from_str(&row.get::<_, String>(6)?).map_err(|error| {
                    rusqlite::Error::FromSqlConversionFailure(
                        6,
                        rusqlite::types::Type::Text,
                        error.into(),
                    )
                })?;
            Ok(RecoveryManifest {
                manifest_id: row.get(0)?,
                project_id: row.get(1)?,
                request_id: row.get(2)?,
                source_digest: row.get(3)?,
                decision_digest: row.get(4)?,
                created_at_ms: row.get::<_, i64>(5)? as u64,
                entries,
            })
        })?;
        let mut manifests = Vec::new();
        for row in rows {
            manifests.push(row?);
        }
        Ok(manifests)
    }

    fn delete_manifest(&mut self, manifest_id: &str) -> Result<()> {
        let connection = self
            .connection
            .lock()
            .map_err(|_| anyhow::anyhow!("bus persistence lock poisoned"))?;
        connection.execute(
            "DELETE FROM bus_manifest WHERE manifest_id = ?1",
            params![manifest_id],
        )?;
        Ok(())
    }

    fn save_grant(&mut self, grant: &SharedGrant) -> Result<()> {
        let connection = self
            .connection
            .lock()
            .map_err(|_| anyhow::anyhow!("bus persistence lock poisoned"))?;
        connection.execute(
            "INSERT INTO bus_grant (
                 grant_id, project_id, owner_agent_id, grantee_agent_id, session_id,
                 grant_revision, granted_at_ms, revoked_at_ms
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
             ON CONFLICT(grant_id) DO UPDATE SET
                 project_id = excluded.project_id,
                 owner_agent_id = excluded.owner_agent_id,
                 grantee_agent_id = excluded.grantee_agent_id,
                 session_id = excluded.session_id,
                 grant_revision = excluded.grant_revision,
                 granted_at_ms = excluded.granted_at_ms,
                 revoked_at_ms = excluded.revoked_at_ms",
            params![
                grant.grant_id,
                grant.project_id,
                grant.owner_agent_id,
                grant.grantee_agent_id,
                grant.session_id,
                grant.grant_revision,
                grant.granted_at_ms as i64,
                grant.revoked_at_ms.map(|value| value as i64),
            ],
        )?;
        Ok(())
    }

    fn load_grants(&self, project_id: &str) -> Result<Vec<SharedGrant>> {
        let connection = self
            .connection
            .lock()
            .map_err(|_| anyhow::anyhow!("bus persistence lock poisoned"))?;
        let mut statement = connection.prepare(
            "SELECT grant_id, project_id, owner_agent_id, grantee_agent_id, session_id,
                    grant_revision, granted_at_ms, revoked_at_ms
             FROM bus_grant WHERE project_id = ?1 ORDER BY granted_at_ms",
        )?;
        let rows = statement.query_map(params![project_id], |row| {
            Ok(SharedGrant {
                grant_id: row.get(0)?,
                project_id: row.get(1)?,
                owner_agent_id: row.get(2)?,
                grantee_agent_id: row.get(3)?,
                session_id: row.get(4)?,
                grant_revision: row.get(5)?,
                granted_at_ms: row.get::<_, i64>(6)? as u64,
                revoked_at_ms: row.get::<_, Option<i64>>(7)?.map(|value| value as u64),
            })
        })?;
        let mut grants = Vec::new();
        for row in rows {
            grants.push(row?);
        }
        Ok(grants)
    }

    fn delete_grant(&mut self, grant_id: &str) -> Result<()> {
        let connection = self
            .connection
            .lock()
            .map_err(|_| anyhow::anyhow!("bus persistence lock poisoned"))?;
        connection.execute("DELETE FROM bus_grant WHERE grant_id = ?1", params![grant_id])?;
        Ok(())
    }

    fn project_ids(&self) -> Result<Vec<String>> {
        let connection = self
            .connection
            .lock()
            .map_err(|_| anyhow::anyhow!("bus persistence lock poisoned"))?;
        let mut statement = connection.prepare(
            "SELECT project_id FROM bus_entry
             UNION SELECT project_id FROM bus_manifest
             UNION SELECT project_id FROM bus_grant",
        )?;
        let rows = statement.query_map([], |row| row.get::<_, String>(0))?;
        let mut ids = Vec::new();
        for row in rows {
            ids.push(row?);
        }
        Ok(ids)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use dtoken_context_bus::signature::{map_signature, source_fingerprint};

    fn entry(project: &str, content: &str) -> CacheEntry {
        CacheEntry {
            map_signature: map_signature(project, content),
            project_id: project.to_string(),
            agent_id: "agent-a".to_string(),
            session_id: "sess-1".to_string(),
            source_fingerprint: source_fingerprint(&"a".repeat(64), None),
            recorded_at_ms: 1_000,
            ttl_seconds: Some(60),
            body_bytes: content.len(),
            query_tokens: vec!["route".to_string()],
            body: Some(format!("candidate:{content}")),
        }
    }

    fn temp_db(label: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "dtoken-bus-persistence-{label}-{}",
            uuid::Uuid::new_v4()
        ))
    }

    #[test]
    fn sqlite_port_roundtrips_and_isolates_by_project() {
        let path = temp_db("roundtrip");
        let mut port = SqlitePersistence::open(&path).unwrap();
        port.save_entry(&entry("proj-a", "one")).unwrap();
        port.save_entry(&entry("proj-b", "two")).unwrap();
        let a = port.load_entries("proj-a").unwrap();
        let b = port.load_entries("proj-b").unwrap();
        assert_eq!(a.len(), 1);
        assert_eq!(b.len(), 1);
        // 隐私边界：持久层不存 body——恢复出的条目是元数据投影。
        assert!(a[0].body.is_none());
        assert_eq!(a[0].map_signature, map_signature("proj-a", "one"));
        assert_eq!(port.load_entries("proj-c").unwrap().len(), 0);
        assert_eq!(port.project_ids().unwrap().len(), 2);
        for suffix in ["", "-wal", "-shm"] {
            let mut sidecar = path.clone().into_os_string();
            sidecar.push(suffix);
            let _ = std::fs::remove_file(sidecar);
        }
    }

    #[test]
    fn sqlite_port_never_persists_plaintext_body() {
        // 安全回归（Main security re-review）：即使 entry 携带完整变换后的
        // prompt/source body，持久层也绝不落盘原文；DB 文件中不出现明文。
        let path = temp_db("nobody");
        let mut port = SqlitePersistence::open(&path).unwrap();
        let plaintext =
            "user: please explain the authentication flow in detail with source code";
        let mut e = entry("proj", "one");
        e.body = Some(plaintext.to_string());
        e.body_bytes = plaintext.len();
        port.save_entry(&e).unwrap();
        port.save_manifest(&RecoveryManifest::new_bound(
            "proj",
            "req-1",
            dtoken_context_bus::signature::sha256_hex(plaintext.as_bytes()),
            "decision-1",
            1_000,
            vec![ManifestEntry::from(&e)],
        ))
        .unwrap();
        drop(port);
        // DB 文件不包含明文 prompt 内容。
        let raw = std::fs::read(&path).unwrap();
        let text = String::from_utf8_lossy(&raw);
        assert!(
            !text.contains("please explain the authentication flow"),
            "plaintext prompt must never be persisted"
        );
        assert!(
            !text.contains(plaintext),
            "transformed body must never be persisted"
        );
        // 重开：元数据恢复可用（body 恒 None）。
        let reopened = SqlitePersistence::open(&path).unwrap();
        let rows = reopened.load_entries("proj").unwrap();
        assert_eq!(rows.len(), 1);
        assert!(rows[0].body.is_none());
        assert_eq!(rows[0].body_bytes, plaintext.len());
        assert_eq!(reopened.load_manifests("proj").unwrap().len(), 1);
        for suffix in ["", "-wal", "-shm"] {
            let mut sidecar = path.clone().into_os_string();
            sidecar.push(suffix);
            let _ = std::fs::remove_file(sidecar);
        }
    }

    #[test]
    fn sqlite_port_migrates_legacy_body_column_and_scrubs_plaintext() {
        // 升级回归（security re-review of e27efa0）：旧 schema（首个集成
        // commit 的 bus_entry 含 body TEXT 列 + 明文行）在 open 时被迁移：
        // body 列移除、明文行不复制、WAL 截断 + secure_delete VACUUM 擦除。
        let path = temp_db("legacy");
        let plaintext = "user: TOP_SECRET_PROMPT_CONTENT_LEGACY";
        let wal_path = {
            let mut p = path.clone().into_os_string();
            p.push("-wal");
            PathBuf::from(p)
        };
        {
            // 种子旧 schema + 明文行；writer 保持打开 → WAL 持有未
            // checkpoint 的明文帧（模拟崩溃/升级前状态）。
            let legacy = Connection::open(&path).unwrap();
            legacy
                .execute_batch(
                    "PRAGMA journal_mode = WAL;
                     CREATE TABLE bus_entry (
                         map_signature TEXT PRIMARY KEY,
                         project_id TEXT NOT NULL,
                         agent_id TEXT NOT NULL,
                         session_id TEXT NOT NULL,
                         source_fingerprint TEXT NOT NULL,
                         recorded_at_ms INTEGER NOT NULL,
                         ttl_seconds INTEGER,
                         body_bytes INTEGER NOT NULL,
                         query_tokens TEXT NOT NULL,
                         body TEXT
                     );
                     INSERT INTO bus_entry VALUES (
                         '1111111111111111111111111111111111111111111111111111111111111111',
                         'proj', 'agent-a', 'sess-1',
                         '2222222222222222222222222222222222222222222222222222222222222222',
                         1000, 60, 33, '[\"route\"]',
                         'TOP_SECRET_PROMPT_CONTENT_LEGACY'
                     );",
                )
                .unwrap();
            // 种子有效性：WAL 文件存在且含明文。
            let wal_raw = std::fs::read(&wal_path).unwrap();
            let wal_text = String::from_utf8_lossy(&wal_raw);
            assert!(
                wal_text.contains("TOP_SECRET_PROMPT_CONTENT_LEGACY"),
                "seed WAL must hold plaintext frames"
            );

            // 第二个连接打开 → 触发迁移（writer 空闲，checkpoint 可完成）。
            let port = SqlitePersistence::open(&path).unwrap();
            // body 列已不存在。
            let check = Connection::open(&path).unwrap();
            let columns: Vec<String> = check
                .prepare("PRAGMA table_info(bus_entry)")
                .unwrap()
                .query_map([], |row| row.get(1))
                .unwrap()
                .collect::<Result<_, _>>()
                .unwrap();
            assert!(
                !columns.iter().any(|name| name == "body"),
                "legacy body column must be migrated away"
            );
            // 元数据恢复可用、body 恒 None。
            let rows = port.load_entries("proj").unwrap();
            assert_eq!(rows.len(), 1);
            assert!(rows[0].body.is_none());
            assert_eq!(rows[0].map_signature, "1".repeat(64));
            drop(port);
            drop(legacy);
        }
        // DB / WAL / SHM 均不含明文（迁移擦除）。
        for suffix in ["", "-wal", "-shm"] {
            let mut p = path.clone().into_os_string();
            p.push(suffix);
            let p = PathBuf::from(p);
            if let Ok(raw) = std::fs::read(&p) {
                let text = String::from_utf8_lossy(&raw);
                assert!(
                    !text.contains("TOP_SECRET_PROMPT_CONTENT_LEGACY"),
                    "plaintext must be scrubbed from {suffix:?}"
                );
            }
        }
        for suffix in ["", "-wal", "-shm"] {
            let mut sidecar = path.clone().into_os_string();
            sidecar.push(suffix);
            let _ = std::fs::remove_file(sidecar);
        }
    }

    #[test]
    fn sqlite_port_delete_semantics() {
        let path = temp_db("delete");
        let mut port = SqlitePersistence::open(&path).unwrap();
        port.save_entry(&entry("proj", "one")).unwrap();
        port.delete_entry("proj", &map_signature("proj", "one"))
            .unwrap();
        assert!(port.load_entries("proj").unwrap().is_empty());
        let manifest = RecoveryManifest::new_bound(
            "proj",
            "req-1",
            dtoken_context_bus::signature::sha256_hex(b"bytes"),
            "decision-1",
            1_000,
            vec![ManifestEntry::from(&entry("proj", "one"))],
        );
        port.save_manifest(&manifest).unwrap();
        assert_eq!(port.load_manifests("proj").unwrap().len(), 1);
        port.delete_manifest(&manifest.manifest_id).unwrap();
        assert!(port.load_manifests("proj").unwrap().is_empty());
        let grant = SharedGrant {
            grant_id: "grant-1".to_string(),
            project_id: "proj".to_string(),
            owner_agent_id: "agent-a".to_string(),
            grantee_agent_id: "agent-b".to_string(),
            session_id: "sess-1".to_string(),
            grant_revision: "rev-1".to_string(),
            granted_at_ms: 1_000,
            revoked_at_ms: None,
        };
        port.save_grant(&grant).unwrap();
        assert_eq!(port.load_grants("proj").unwrap().len(), 1);
        port.delete_grant("grant-1").unwrap();
        assert!(port.load_grants("proj").unwrap().is_empty());
        for suffix in ["", "-wal", "-shm"] {
            let mut sidecar = path.clone().into_os_string();
            sidecar.push(suffix);
            let _ = std::fs::remove_file(sidecar);
        }
    }

    #[test]
    fn sqlite_port_roundtrips_manifest_binding_and_revocation() {
        let path = temp_db("binding");
        let mut port = SqlitePersistence::open(&path).unwrap();
        let manifest = RecoveryManifest::new_bound(
            "proj",
            "req-9",
            "deadbeef".repeat(8),
            "decision-9",
            2_000,
            vec![ManifestEntry::from(&entry("proj", "one"))],
        );
        port.save_manifest(&manifest).unwrap();
        let loaded = port.load_manifests("proj").unwrap();
        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded[0].request_id, "req-9");
        assert!(loaded[0].binding_matches("req-9", &"deadbeef".repeat(8)));
        let grant = SharedGrant {
            grant_id: "grant-2".to_string(),
            project_id: "proj".to_string(),
            owner_agent_id: "agent-a".to_string(),
            grantee_agent_id: "agent-b".to_string(),
            session_id: "sess-1".to_string(),
            grant_revision: "rev-1".to_string(),
            granted_at_ms: 1_000,
            revoked_at_ms: Some(2_000),
        };
        port.save_grant(&grant).unwrap();
        assert_eq!(
            port.load_grants("proj").unwrap()[0].revoked_at_ms,
            Some(2_000)
        );
        for suffix in ["", "-wal", "-shm"] {
            let mut sidecar = path.clone().into_os_string();
            sidecar.push(suffix);
            let _ = std::fs::remove_file(sidecar);
        }
    }

    #[test]
    fn sqlite_port_rejects_symlink_path() {
        // 与 ContextStore 一致：路径必须是 regular file，拒绝 symlink。
        let base = temp_db("symlink");
        let target = std::env::temp_dir().join(format!(
            "dtoken-bus-persistence-target-{}",
            uuid::Uuid::new_v4()
        ));
        let port = SqlitePersistence::open(&target).unwrap();
        drop(port);
        std::fs::remove_file(&target).ok();
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(&target, &base).ok();
            assert!(SqlitePersistence::open(&base).is_err());
        }
        let _ = std::fs::remove_file(&base);
        let _ = std::fs::remove_file(&target);
    }
}

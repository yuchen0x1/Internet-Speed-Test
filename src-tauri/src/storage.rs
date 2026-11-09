//! 本地数据存储:SQLite(WAL)历史记录库 + 节点收藏。
//!
//! 历史记录仅来自实际执行的测试(completed / cancelled / failed 如实标注)。

use std::collections::HashMap;
use std::path::Path;
use std::sync::{Arc, Mutex};

use rusqlite::Connection;

use crate::engine::model::TestResult;

#[derive(Debug, Clone, serde::Serialize)]
pub struct HistoryRow {
    pub id: i64,
    pub created_at: String,
    pub download_mbps: f64,
    pub upload_mbps: Option<f64>,
    pub idle_ping_ms: f64,
    pub jitter_ms: f64,
    pub loaded_down_ms: Option<f64>,
    pub loaded_up_ms: Option<f64>,
    pub packet_loss_percent: Option<f64>,
    pub server_name: String,
    pub server_provider: String,
    pub server_url: String,
    pub network_type: String,
    pub interface_name: String,
    pub public_ip: Option<String>,
    pub isp: Option<String>,
    pub status: String,
}

pub struct Storage {
    conn: Arc<Mutex<Connection>>,
}

/// 在阻塞线程池写入历史:async 调用方(引擎收尾)不占用 tokio 工作线程
pub async fn insert_result_blocking(
    storage: &Arc<Storage>,
    r: &TestResult,
) -> Result<(), String> {
    let s = Arc::clone(storage);
    let r = r.clone();
    tokio::task::spawn_blocking(move || s.insert_result(&r))
        .await
        .map_err(|e| e.to_string())?
}

fn sanitize_legacy_ndt7_details(conn: &Connection) -> Result<usize, String> {
    let mut stmt = conn
        .prepare(
            "SELECT id, detail FROM history WHERE server_provider = 'ndt7' AND detail IS NOT NULL",
        )
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map([], |row| {
            Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?))
        })
        .map_err(|e| e.to_string())?;
    let mut sanitized = Vec::new();
    for row in rows {
        let (id, text) = row.map_err(|e| e.to_string())?;
        let Ok(mut result) = serde_json::from_str::<TestResult>(&text) else {
            // 无法解析的 NDT7 详情不应继续保留潜在访问令牌。
            sanitized.push((id, None));
            continue;
        };
        let Some(node) = result.server.as_mut().filter(|n| n.provider == "ndt7") else {
            continue;
        };
        if node.dl_url.is_empty() && node.ul_url.is_empty() && node.ping_url.is_empty() {
            continue;
        }
        node.dl_url.clear();
        node.ul_url.clear();
        node.ping_url.clear();
        let detail = serde_json::to_string(&result).map_err(|e| e.to_string())?;
        sanitized.push((id, Some(detail)));
    }
    drop(stmt);
    for (id, detail) in &sanitized {
        conn.execute(
            "UPDATE history SET detail = ?1 WHERE id = ?2",
            rusqlite::params![detail, id],
        )
        .map_err(|e| e.to_string())?;
    }
    Ok(sanitized.len())
}

impl Storage {
    pub fn open(db_path: &Path) -> Result<Self, String> {
        if let Some(dir) = db_path.parent() {
            std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
        }
        let conn = Connection::open(db_path).map_err(|e| e.to_string())?;
        conn.pragma_update(None, "journal_mode", "WAL")
            .map_err(|e| e.to_string())?;
        conn.pragma_update(None, "synchronous", "NORMAL")
            .map_err(|e| e.to_string())?;
        // CLI 与桌面端可同时打开本库,等待锁而不是立即报 SQLITE_BUSY
        conn.busy_timeout(std::time::Duration::from_secs(5))
            .map_err(|e| e.to_string())?;
        // 版本化迁移:v1 为初始 schema;后续加列在 version < N 分支中依次升级
        let user_version: i64 = conn
            .query_row("PRAGMA user_version", [], |row| row.get(0))
            .map_err(|e| e.to_string())?;
        if user_version < 1 {
            conn.execute_batch(
                "CREATE TABLE IF NOT EXISTS history (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                created_at TEXT NOT NULL,
                download_mbps REAL NOT NULL DEFAULT 0,
                upload_mbps REAL,
                idle_ping_ms REAL NOT NULL DEFAULT 0,
                jitter_ms REAL NOT NULL DEFAULT 0,
                loaded_down_ms REAL,
                loaded_up_ms REAL,
                packet_loss_percent REAL,
                server_name TEXT NOT NULL DEFAULT '',
                server_provider TEXT NOT NULL DEFAULT '',
                server_url TEXT NOT NULL DEFAULT '',
                network_type TEXT,
                interface_name TEXT,
                public_ip TEXT,
                isp TEXT,
                status TEXT NOT NULL DEFAULT 'completed',
                detail TEXT
            );
            CREATE INDEX IF NOT EXISTS idx_history_created ON history(created_at DESC);
            CREATE TABLE IF NOT EXISTS favorite_servers (
                url TEXT PRIMARY KEY,
                name TEXT NOT NULL DEFAULT '',
                added_at TEXT NOT NULL DEFAULT ''
            );",
            )
            .map_err(|e| e.to_string())?;
            conn.pragma_update(None, "user_version", 1)
                .map_err(|e| e.to_string())?;
        }
        conn.pragma_update(None, "secure_delete", "ON")
            .map_err(|e| e.to_string())?;
        let sanitized = sanitize_legacy_ndt7_details(&conn)?;
        if sanitized > 0 {
            conn.execute_batch("VACUUM;").map_err(|e| e.to_string())?;
            conn.execute_batch("PRAGMA wal_checkpoint(TRUNCATE);")
                .map_err(|e| e.to_string())?;
        }
        Ok(Self {
            conn: Arc::new(Mutex::new(conn)),
        })
    }

    pub fn insert_result(&self, r: &TestResult) -> Result<(), String> {
        let detail = serde_json::to_string(r).map_err(|e| e.to_string())?;
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO history (created_at, download_mbps, upload_mbps, idle_ping_ms, jitter_ms,
                loaded_down_ms, loaded_up_ms, packet_loss_percent, server_name, server_provider,
                server_url, network_type, interface_name, public_ip, isp, status, detail)
             VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,?17)",
            rusqlite::params![
                r.started_at,
                r.download_mbps,
                r.upload_mbps,
                r.idle_ping_ms,
                r.jitter_ms,
                r.loaded_down_ms,
                r.loaded_up_ms,
                r.packet_loss_percent,
                r.server
                    .as_ref()
                    .map(|s| s.name.clone())
                    .unwrap_or_default(),
                r.server
                    .as_ref()
                    .map(|s| s.provider.clone())
                    .unwrap_or_default(),
                r.server
                    .as_ref()
                    .map(|s| s.base_url.clone())
                    .unwrap_or_default(),
                r.network_type,
                r.interface_name,
                r.public_ip,
                r.isp,
                r.status,
                detail,
            ],
        )
        .map_err(|e| e.to_string())?;
        Ok(())
    }

    pub fn list_history(
        &self,
        from: Option<String>,
        to: Option<String>,
        order_desc: bool,
        limit: u32,
    ) -> Result<Vec<HistoryRow>, String> {
        let conn = self.conn.lock().unwrap();
        let mut sql = String::from(
            "SELECT id, created_at, download_mbps, upload_mbps, idle_ping_ms, jitter_ms,
                    loaded_down_ms, loaded_up_ms, packet_loss_percent, server_name,
                    server_provider, server_url, network_type, interface_name, public_ip, isp, status
             FROM history WHERE 1=1",
        );
        let mut params: Vec<Box<dyn rusqlite::types::ToSql>> = Vec::new();
        if let Some(f) = &from {
            sql.push_str(&format!(" AND created_at >= ?{}", params.len() + 1));
            params.push(Box::new(f.clone()));
        }
        if let Some(t) = &to {
            sql.push_str(&format!(" AND created_at <= ?{}", params.len() + 1));
            params.push(Box::new(t.clone()));
        }
        sql.push_str(if order_desc {
            " ORDER BY id DESC LIMIT ?"
        } else {
            " ORDER BY id ASC LIMIT ?"
        });
        params.push(Box::new(limit.clamp(1, 5000)));
        let mut stmt = conn.prepare(&sql).map_err(|e| e.to_string())?;
        let refs: Vec<&dyn rusqlite::types::ToSql> = params.iter().map(|p| p.as_ref()).collect();
        let rows = stmt
            .query_map(refs.as_slice(), |row| {
                Ok(HistoryRow {
                    id: row.get(0)?,
                    created_at: row.get(1)?,
                    download_mbps: row.get(2)?,
                    upload_mbps: row.get(3)?,
                    idle_ping_ms: row.get(4)?,
                    jitter_ms: row.get(5)?,
                    loaded_down_ms: row.get(6)?,
                    loaded_up_ms: row.get(7)?,
                    packet_loss_percent: row.get(8)?,
                    server_name: row.get(9)?,
                    server_provider: row.get(10)?,
                    server_url: row.get(11)?,
                    network_type: row.get(12)?,
                    interface_name: row.get(13)?,
                    public_ip: row.get(14)?,
                    isp: row.get(15)?,
                    status: row.get(16)?,
                })
            })
            .map_err(|e| e.to_string())?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(|e| e.to_string())
    }

    pub fn get_result_detail(&self, id: i64) -> Result<Option<TestResult>, String> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn
            .prepare("SELECT detail FROM history WHERE id = ?1")
            .map_err(|e| e.to_string())?;
        let mut rows = stmt.query([id]).map_err(|e| e.to_string())?;
        match rows.next().map_err(|e| e.to_string())? {
            Some(row) => {
                let text: Option<String> = row.get(0).map_err(|e| e.to_string())?;
                Ok(text.and_then(|t| serde_json::from_str(&t).ok()))
            }
            None => Ok(None),
        }
    }

    pub fn delete_history(&self, id: i64) -> Result<(), String> {
        self.conn
            .lock()
            .unwrap()
            .execute("DELETE FROM history WHERE id = ?1", [id])
            .map_err(|e| e.to_string())?;
        Ok(())
    }

    /// 保留策略:删除 created_at 早于 cutoff(含)之前的记录,返回删除数
    pub fn cleanup_retention(&self, days: u32) -> Result<u64, String> {
        if days == 0 {
            return Ok(0); // 0 = 永久保留
        }
        let cutoff = chrono::Local::now() - chrono::Duration::days(days as i64);
        let cutoff_str = cutoff.format("%Y-%m-%d %H:%M:%S").to_string();
        let n = self
            .conn
            .lock()
            .unwrap()
            .execute("DELETE FROM history WHERE created_at < ?1", [cutoff_str])
            .map_err(|e| e.to_string())?;
        Ok(n as u64)
    }

    pub fn clear_history(&self) -> Result<(), String> {
        self.conn
            .lock()
            .unwrap()
            .execute("DELETE FROM history", [])
            .map_err(|e| e.to_string())?;
        Ok(())
    }

    pub fn history_count(&self) -> Result<u32, String> {
        self.conn
            .lock()
            .unwrap()
            .query_row("SELECT COUNT(*) FROM history", [], |r| r.get(0))
            .map_err(|e| e.to_string())
    }

    // ---------- 节点收藏 ----------

    pub fn favorites_map(&self) -> HashMap<String, bool> {
        let conn = self.conn.lock().unwrap();
        let mut map = HashMap::new();
        if let Ok(mut stmt) = conn.prepare("SELECT url FROM favorite_servers") {
            if let Ok(rows) = stmt.query_map([], |r| r.get::<_, String>(0)) {
                for url in rows.flatten() {
                    map.insert(url, true);
                }
            }
        }
        map
    }

    pub fn set_favorite(&self, url: &str, name: &str, fav: bool) -> Result<(), String> {
        let conn = self.conn.lock().unwrap();
        if fav {
            conn.execute(
                "INSERT OR REPLACE INTO favorite_servers (url, name, added_at) VALUES (?1, ?2, ?3)",
                rusqlite::params![
                    url,
                    name,
                    chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string()
                ],
            )
            .map_err(|e| e.to_string())?;
        } else {
            conn.execute("DELETE FROM favorite_servers WHERE url = ?1", [url])
                .map_err(|e| e.to_string())?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::model::ServerNodeInfo;

    fn mem_storage() -> Storage {
        // rusqlite 内存库:路径 ":memory:" 走同一 open 逻辑但跳过目录
        let conn = Connection::open_in_memory().unwrap();
        conn.pragma_update(None, "journal_mode", "WAL").unwrap();
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS history (
                id INTEGER PRIMARY KEY AUTOINCREMENT, created_at TEXT NOT NULL,
                download_mbps REAL NOT NULL DEFAULT 0, upload_mbps REAL,
                idle_ping_ms REAL NOT NULL DEFAULT 0, jitter_ms REAL NOT NULL DEFAULT 0,
                loaded_down_ms REAL, loaded_up_ms REAL, packet_loss_percent REAL,
                server_name TEXT NOT NULL DEFAULT '', server_provider TEXT NOT NULL DEFAULT '',
                server_url TEXT NOT NULL DEFAULT '', network_type TEXT, interface_name TEXT,
                public_ip TEXT, isp TEXT, status TEXT NOT NULL DEFAULT 'completed', detail TEXT);
            CREATE TABLE IF NOT EXISTS favorite_servers (
                url TEXT PRIMARY KEY, name TEXT NOT NULL DEFAULT '', added_at TEXT NOT NULL DEFAULT '');",
        )
        .unwrap();
        Storage {
            conn: Arc::new(Mutex::new(conn)),
        }
    }

    fn sample_result(status: &str) -> TestResult {
        TestResult {
            id: "test1".into(),
            started_at: "2026-11-07 10:00:00".into(),
            finished_at: "2026-11-07 10:00:35".into(),
            status: status.into(),
            download_mbps: 125.4,
            upload_mbps: Some(45.2),
            idle_ping_ms: 18.5,
            jitter_ms: 2.3,
            packet_loss_percent: Some(0.0),
            server: Some(ServerNodeInfo {
                name: "Test Node".into(),
                provider: "librespeed".into(),
                base_url: "https://test.example.com/backend".into(),
                ..Default::default()
            }),
            network_type: "ethernet".into(),
            ..Default::default()
        }
    }

    #[test]
    fn test_legacy_ndt7_history_token_is_removed() {
        let s = mem_storage();
        let mut result = sample_result("completed");
        result.server = Some(ServerNodeInfo {
            provider: "ndt7".into(),
            dl_url: "wss://mlab.example/download?access_token=legacy-secret".into(),
            ul_url: "wss://mlab.example/upload?access_token=legacy-secret".into(),
            ping_url: "wss://mlab.example/download?access_token=legacy-secret".into(),
            ..Default::default()
        });
        let mut legacy = serde_json::to_value(&result).unwrap();
        let server = legacy.get_mut("server").unwrap();
        server["dl_url"] =
            serde_json::json!("wss://mlab.example/download?access_token=legacy-secret");
        server["ul_url"] =
            serde_json::json!("wss://mlab.example/upload?access_token=legacy-secret");
        server["ping_url"] =
            serde_json::json!("wss://mlab.example/download?access_token=legacy-secret");
        let raw = serde_json::to_string(&legacy).unwrap();
        s.conn
            .lock()
            .unwrap()
            .execute(
                "INSERT INTO history (created_at, server_provider, detail) VALUES (?1, 'ndt7', ?2)",
                rusqlite::params![result.started_at, raw],
            )
            .unwrap();
        assert_eq!(
            sanitize_legacy_ndt7_details(&s.conn.lock().unwrap()).unwrap(),
            1
        );
        let stored: String = s
            .conn
            .lock()
            .unwrap()
            .query_row(
                "SELECT detail FROM history WHERE server_provider = 'ndt7'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert!(!stored.contains("legacy-secret"));
    }

    #[test]
    fn test_history_crud() {
        let s = mem_storage();
        s.insert_result(&sample_result("completed")).unwrap();
        s.insert_result(&sample_result("cancelled")).unwrap();
        let list = s.list_history(None, None, true, 100).unwrap();
        assert_eq!(list.len(), 2);
        assert_eq!(list[0].status, "cancelled");
        assert!((list[1].download_mbps - 125.4).abs() < 1e-9);
        assert_eq!(list[1].server_name, "Test Node");

        let id = list[1].id;
        let detail = s.get_result_detail(id).unwrap().unwrap();
        assert!((detail.download_mbps - 125.4).abs() < 1e-9);

        s.delete_history(id).unwrap();
        assert_eq!(s.list_history(None, None, true, 100).unwrap().len(), 1);
        s.clear_history().unwrap();
        assert_eq!(s.history_count().unwrap(), 0);
    }

    #[test]
    fn test_favorites() {
        let s = mem_storage();
        assert!(s.favorites_map().is_empty());
        s.set_favorite("https://a.com", "Node A", true).unwrap();
        assert!(s.favorites_map().contains_key("https://a.com"));
        s.set_favorite("https://a.com", "Node A", false).unwrap();
        assert!(s.favorites_map().is_empty());
    }

    #[test]
    fn test_filter_by_date() {
        let s = mem_storage();
        s.insert_result(&sample_result("completed")).unwrap();
        let hit = s
            .list_history(
                Some("2026-11-01".into()),
                Some("2026-11-30".into()),
                true,
                10,
            )
            .unwrap();
        assert_eq!(hit.len(), 1);
        let miss = s
            .list_history(
                Some("2020-01-01".into()),
                Some("2020-01-02".into()),
                true,
                10,
            )
            .unwrap();
        assert!(miss.is_empty());
    }
}

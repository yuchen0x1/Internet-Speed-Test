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

fn history_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<HistoryRow> {
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
}

pub struct HistoryMetrics {
    pub latest: Option<HistoryRow>,
    pub total: u32,
    pub completed: u32,
    pub failed: u32,
    pub cancelled: u32,
}

/// SQL 聚合整个筛选范围，不受列表分页或导出上限影响。
#[derive(Debug, Clone, serde::Serialize)]
pub struct HistorySummary {
    pub total: u32,
    pub completed: u32,
    pub failed: u32,
    pub cancelled: u32,
    pub avg_download_mbps: Option<f64>,
    pub avg_upload_mbps: Option<f64>,
    pub avg_ping_ms: Option<f64>,
    pub best_download_mbps: Option<f64>,
}

/// 日期与库中本地时间一致。日期上界包含整天，旧时间戳调用保留精确边界。
fn history_bounds(
    from: Option<String>,
    to: Option<String>,
) -> Result<(Option<String>, Option<String>), String> {
    fn bound(value: Option<String>, end: bool) -> Result<Option<String>, String> {
        let Some(value) = value.filter(|s| !s.is_empty()) else {
            return Ok(None);
        };
        if value.len() == 10 {
            let date = chrono::NaiveDate::parse_from_str(&value, "%Y-%m-%d")
                .map_err(|_| "无效日期，请使用 YYYY-MM-DD".to_string())?;
            if date.format("%Y-%m-%d").to_string() != value {
                return Err("无效日期，请使用 YYYY-MM-DD".into());
            }
            return Ok(Some(format!(
                "{value} {}",
                if end {
                    "23:59:59.999999999"
                } else {
                    "00:00:00"
                }
            )));
        }
        chrono::NaiveDateTime::parse_from_str(&value, "%Y-%m-%d %H:%M:%S%.f")
            .map_err(|_| "无效日期时间，请使用 YYYY-MM-DD HH:MM:SS".to_string())?;
        Ok(Some(value))
    }
    let from = bound(from, false)?;
    let to = bound(to, true)?;
    if from.as_ref().zip(to.as_ref()).is_some_and(|(f, t)| f > t) {
        return Err("起始日期不能晚于结束日期".into());
    }
    Ok((from, to))
}

/// 诊断历史的列表行(不含完整 detail,避免列表查询搬运大 JSON)
#[derive(Debug, Clone, serde::Serialize)]
pub struct DiagRunRow {
    pub id: i64,
    pub created_at: String,
    /// 诊断类型:traceroute / ping / dns / https / route_quality / nat / ...
    pub kind: String,
    pub target: Option<String>,
    pub ok: bool,
    pub summary: String,
}

/// 待写入的诊断记录(借用,避免克隆大字符串)
pub struct DiagRunInput<'a> {
    pub created_at: &'a str,
    pub kind: &'a str,
    pub target: Option<&'a str>,
    pub ok: bool,
    pub summary: &'a str,
    pub detail: Option<&'a str>,
}

pub struct Storage {
    conn: Arc<Mutex<Connection>>,
}

/// 在阻塞线程池写入历史:async 调用方(引擎收尾)不占用 tokio 工作线程
pub async fn insert_result_blocking(storage: &Arc<Storage>, r: &TestResult) -> Result<(), String> {
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
        // v2:网络诊断历史。设置项 save_diag_history 真实生效的依据。
        if user_version < 2 {
            conn.execute_batch(
                "CREATE TABLE IF NOT EXISTS diag_runs (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                created_at TEXT NOT NULL,
                kind TEXT NOT NULL,
                target TEXT,
                ok INTEGER NOT NULL DEFAULT 0,
                summary TEXT NOT NULL DEFAULT '',
                detail TEXT
            );
            CREATE INDEX IF NOT EXISTS idx_diag_created ON diag_runs(created_at DESC);
            CREATE INDEX IF NOT EXISTS idx_diag_kind ON diag_runs(kind, created_at DESC);",
            )
            .map_err(|e| e.to_string())?;
            conn.pragma_update(None, "user_version", 2)
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
        let conn = crate::safe_lock::lock(&self.conn);
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
        self.list_history_page(from, to, order_desc, limit, None)
    }

    pub fn list_history_page(
        &self,
        from: Option<String>,
        to: Option<String>,
        order_desc: bool,
        limit: u32,
        cursor_id: Option<i64>,
    ) -> Result<Vec<HistoryRow>, String> {
        let (from, to) = history_bounds(from, to)?;
        let conn = crate::safe_lock::lock(&self.conn);
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
        if let Some(cursor_id) = cursor_id {
            let comparator = if order_desc { "<" } else { ">" };
            sql.push_str(&format!(" AND id {comparator} ?{}", params.len() + 1));
            params.push(Box::new(cursor_id));
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
            .query_map(refs.as_slice(), history_row)
            .map_err(|e| e.to_string())?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(|e| e.to_string())
    }

    pub fn get_result_detail(&self, id: i64) -> Result<Option<TestResult>, String> {
        let conn = crate::safe_lock::lock(&self.conn);
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
        crate::safe_lock::lock(&self.conn)
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
        let n = crate::safe_lock::lock(&self.conn)
            .execute("DELETE FROM history WHERE created_at < ?1", [cutoff_str])
            .map_err(|e| e.to_string())?;
        Ok(n as u64)
    }

    pub fn clear_history(&self) -> Result<(), String> {
        crate::safe_lock::lock(&self.conn)
            .execute("DELETE FROM history", [])
            .map_err(|e| e.to_string())?;
        Ok(())
    }

    #[cfg(test)]
    pub fn history_count(&self) -> Result<u32, String> {
        crate::safe_lock::lock(&self.conn)
            .query_row("SELECT COUNT(*) FROM history", [], |r| r.get(0))
            .map_err(|e| e.to_string())
    }

    /// One connection lock keeps counts and the latest successful result consistent.
    #[cfg(test)]
    pub(crate) fn invalidate_metrics_storage_for_test(&self) {
        crate::safe_lock::lock(&self.conn)
            .execute("DROP TABLE history", [])
            .unwrap();
    }

    pub fn metrics_snapshot(&self) -> Result<HistoryMetrics, String> {
        use rusqlite::OptionalExtension;
        let conn = crate::safe_lock::lock(&self.conn);
        let (total, completed, failed, cancelled) = conn.query_row(
            "SELECT COUNT(*), COUNT(CASE WHEN status='completed' THEN 1 END),
                COUNT(CASE WHEN status='failed' THEN 1 END), COUNT(CASE WHEN status='cancelled' THEN 1 END) FROM history",
            [], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
        ).map_err(|e| e.to_string())?;
        let latest = conn
            .query_row(
                "SELECT id, created_at, download_mbps, upload_mbps, idle_ping_ms, jitter_ms,
                loaded_down_ms, loaded_up_ms, packet_loss_percent, server_name,
                server_provider, server_url, network_type, interface_name, public_ip, isp, status
             FROM history WHERE status='completed' ORDER BY id DESC LIMIT 1",
                [],
                history_row,
            )
            .optional()
            .map_err(|e| e.to_string())?;
        Ok(HistoryMetrics {
            latest,
            total,
            completed,
            failed,
            cancelled,
        })
    }

    /// 满足筛选条件的历史条数(与 `list_history` 同一 WHERE 口径)。
    /// 用于前端区分"已加载 M 条"和"库内共 N 条",避免把窗口内统计当成全量。
    pub fn history_count_filtered(
        &self,
        from: Option<String>,
        to: Option<String>,
    ) -> Result<u32, String> {
        let (from, to) = history_bounds(from, to)?;
        let conn = crate::safe_lock::lock(&self.conn);
        let mut sql = String::from("SELECT COUNT(*) FROM history WHERE 1=1");
        let mut params: Vec<Box<dyn rusqlite::types::ToSql>> = Vec::new();
        if let Some(f) = &from {
            sql.push_str(&format!(" AND created_at >= ?{}", params.len() + 1));
            params.push(Box::new(f.clone()));
        }
        if let Some(t) = &to {
            sql.push_str(&format!(" AND created_at <= ?{}", params.len() + 1));
            params.push(Box::new(t.clone()));
        }
        let refs: Vec<&dyn rusqlite::types::ToSql> = params.iter().map(|p| p.as_ref()).collect();
        conn.query_row(&sql, refs.as_slice(), |r| r.get(0))
            .map_err(|e| e.to_string())
    }

    pub fn history_summary(
        &self,
        from: Option<String>,
        to: Option<String>,
    ) -> Result<HistorySummary, String> {
        let (from, to) = history_bounds(from, to)?;
        let conn = crate::safe_lock::lock(&self.conn);
        conn.query_row(
            "SELECT COUNT(*),
                COUNT(CASE WHEN status = 'completed' THEN 1 END),
                COUNT(CASE WHEN status = 'failed' THEN 1 END),
                COUNT(CASE WHEN status = 'cancelled' THEN 1 END),
                AVG(CASE WHEN status = 'completed' THEN download_mbps END),
                AVG(CASE WHEN status = 'completed' THEN upload_mbps END),
                AVG(CASE WHEN status = 'completed' THEN idle_ping_ms END),
                MAX(CASE WHEN status = 'completed' THEN download_mbps END)
             FROM history WHERE (?1 IS NULL OR created_at >= ?1)
                AND (?2 IS NULL OR created_at <= ?2)",
            rusqlite::params![from, to],
            |r| {
                Ok(HistorySummary {
                    total: r.get(0)?,
                    completed: r.get(1)?,
                    failed: r.get(2)?,
                    cancelled: r.get(3)?,
                    avg_download_mbps: r.get(4)?,
                    avg_upload_mbps: r.get(5)?,
                    avg_ping_ms: r.get(6)?,
                    best_download_mbps: r.get(7)?,
                })
            },
        )
        .map_err(|e| e.to_string())
    }

    // ---------- 网络诊断历史 ----------

    /// 记录一次诊断。只有设置开启时调用方才会落库。
    /// `detail` 保存完整结构化结果,列表只取摘要字段以免大字段拖慢渲染。
    pub fn insert_diag_run(&self, run: &DiagRunInput<'_>) -> Result<i64, String> {
        let conn = crate::safe_lock::lock(&self.conn);
        conn.execute(
            "INSERT INTO diag_runs (created_at, kind, target, ok, summary, detail)
             VALUES (?1,?2,?3,?4,?5,?6)",
            rusqlite::params![
                run.created_at,
                run.kind,
                run.target,
                if run.ok { 1 } else { 0 },
                run.summary,
                run.detail,
            ],
        )
        .map_err(|e| e.to_string())?;
        Ok(conn.last_insert_rowid())
    }

    pub fn list_diag_runs(
        &self,
        kind: Option<&str>,
        limit: u32,
    ) -> Result<Vec<DiagRunRow>, String> {
        let conn = crate::safe_lock::lock(&self.conn);
        let limit = limit.clamp(1, 1000);
        let mut out = Vec::new();
        let map = |row: &rusqlite::Row<'_>| {
            Ok(DiagRunRow {
                id: row.get(0)?,
                created_at: row.get(1)?,
                kind: row.get(2)?,
                target: row.get(3)?,
                ok: row.get::<_, i64>(4)? != 0,
                summary: row.get(5)?,
            })
        };
        if let Some(k) = kind {
            let mut stmt = conn
                .prepare(
                    "SELECT id, created_at, kind, target, ok, summary FROM diag_runs
                     WHERE kind = ?1 ORDER BY id DESC LIMIT ?2",
                )
                .map_err(|e| e.to_string())?;
            let rows = stmt
                .query_map(rusqlite::params![k, limit], map)
                .map_err(|e| e.to_string())?;
            for r in rows {
                out.push(r.map_err(|e| e.to_string())?);
            }
        } else {
            let mut stmt = conn
                .prepare(
                    "SELECT id, created_at, kind, target, ok, summary FROM diag_runs
                     ORDER BY id DESC LIMIT ?1",
                )
                .map_err(|e| e.to_string())?;
            let rows = stmt.query_map([limit], map).map_err(|e| e.to_string())?;
            for r in rows {
                out.push(r.map_err(|e| e.to_string())?);
            }
        }
        Ok(out)
    }

    pub fn diag_run_detail(&self, id: i64) -> Result<Option<String>, String> {
        let conn = crate::safe_lock::lock(&self.conn);
        let mut stmt = conn
            .prepare("SELECT detail FROM diag_runs WHERE id = ?1")
            .map_err(|e| e.to_string())?;
        let mut rows = stmt.query([id]).map_err(|e| e.to_string())?;
        match rows.next().map_err(|e| e.to_string())? {
            Some(row) => Ok(row.get::<_, Option<String>>(0).map_err(|e| e.to_string())?),
            None => Ok(None),
        }
    }

    pub fn clear_diag_runs(&self) -> Result<u64, String> {
        let conn = crate::safe_lock::lock(&self.conn);
        let n = conn
            .execute("DELETE FROM diag_runs", [])
            .map_err(|e| e.to_string())?;
        Ok(n as u64)
    }

    /// 与历史保留策略共用清理(诊断记录不单独设保留期)
    pub fn cleanup_diag_runs(&self, days: u32) -> Result<u64, String> {
        if days == 0 {
            return Ok(0);
        }
        let conn = crate::safe_lock::lock(&self.conn);
        let n = conn
            .execute(
                "DELETE FROM diag_runs WHERE created_at < datetime('now','localtime',?1)",
                rusqlite::params![format!("-{days} days")],
            )
            .map_err(|e| e.to_string())?;
        Ok(n as u64)
    }

    // ---------- 节点收藏 ----------

    pub fn favorites_map(&self) -> HashMap<String, bool> {
        let conn = crate::safe_lock::lock(&self.conn);
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
        let conn = crate::safe_lock::lock(&self.conn);
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
    fn test_history_pagination_preserves_order_and_filter() {
        let s = mem_storage();
        for _ in 0..5 {
            s.insert_result(&sample_result("completed")).unwrap();
        }
        let all = s
            .list_history_page(
                Some("2026-11-01".into()),
                Some("2026-11-30".into()),
                true,
                10,
                None,
            )
            .unwrap();
        let page1 = s
            .list_history_page(
                Some("2026-11-01".into()),
                Some("2026-11-30".into()),
                true,
                2,
                None,
            )
            .unwrap();
        let page2 = s
            .list_history_page(
                Some("2026-11-01".into()),
                Some("2026-11-30".into()),
                true,
                2,
                page1.last().map(|row| row.id),
            )
            .unwrap();
        let page3 = s
            .list_history_page(
                Some("2026-11-01".into()),
                Some("2026-11-30".into()),
                true,
                2,
                page2.last().map(|row| row.id),
            )
            .unwrap();
        let joined: Vec<i64> = page1
            .iter()
            .chain(&page2)
            .chain(&page3)
            .map(|row| row.id)
            .collect();
        assert_eq!(joined, all.iter().map(|row| row.id).collect::<Vec<_>>());
        assert_eq!(all.len(), 5);
        assert_eq!(page3.len(), 1);

        let asc = s.list_history_page(None, None, false, 2, None).unwrap();
        let asc_next = s
            .list_history_page(None, None, false, 2, asc.last().map(|row| row.id))
            .unwrap();
        assert!(asc[0].id < asc[1].id);
        assert!(asc_next[0].id > asc[1].id);

        s.insert_result(&sample_result("completed")).unwrap();
        let older_than_page1 = s
            .list_history_page(None, None, true, 10, page1.last().map(|row| row.id))
            .unwrap();
        assert_eq!(older_than_page1.len(), 3, "分页游标不受新插入最新记录影响");
        let mut disjoint = page1.iter().map(|row| row.id).collect::<Vec<_>>();
        disjoint.extend(older_than_page1.iter().map(|row| row.id));
        disjoint.sort_unstable();
        disjoint.dedup();
        assert_eq!(disjoint.len(), 5, "游标分页无重复且不漏掉已有旧记录");

        s.delete_history(page1[1].id).unwrap();
        let still_after_cursor = s
            .list_history_page(None, None, true, 10, page1.last().map(|row| row.id))
            .unwrap();
        assert_eq!(still_after_cursor.len(), 3, "删除较新记录不移动游标边界");
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

    #[test]
    fn test_calendar_range_includes_end_day_in_list_count_and_summary() {
        let s = mem_storage();
        for time in [
            "2026-11-06 23:59:59",
            "2026-11-07 00:00:00",
            "2026-11-07 23:59:59.123",
            "2026-11-08 00:00:00",
        ] {
            let mut result = sample_result("completed");
            result.started_at = time.into();
            s.insert_result(&result).unwrap();
        }
        let from = Some("2026-11-07".into());
        let to = from.clone();
        let rows = s
            .list_history(from.clone(), to.clone(), true, 5000)
            .unwrap();
        assert_eq!(rows.len(), 2, "同一天的起止日期必须包含午夜和深夜");
        assert_eq!(
            s.history_count_filtered(from.clone(), to.clone()).unwrap(),
            2
        );
        assert_eq!(s.history_summary(from, to).unwrap().total, 2);
        assert_eq!(
            s.list_history(None, Some("2026-11-07 12:00:00".into()), true, 5000)
                .unwrap()
                .len(),
            2,
            "旧调用方的精确时间上界不应扩成整天"
        );
    }

    #[test]
    fn test_history_range_rejects_invalid_or_inverted_dates() {
        let s = mem_storage();
        for (from, to) in [
            ("2026-02-29", "2026-03-01"),
            ("2026-11-08", "2026-11-07"),
            ("invalid", "2026-11-07"),
        ] {
            assert!(s
                .list_history(Some(from.into()), Some(to.into()), true, 10)
                .is_err());
            assert!(s
                .history_count_filtered(Some(from.into()), Some(to.into()))
                .is_err());
            assert!(s
                .history_summary(Some(from.into()), Some(to.into()))
                .is_err());
        }
        assert!(history_bounds(Some("2028-02-29".into()), None).is_ok());
        assert_eq!(
            history_bounds(Some(String::new()), None).unwrap(),
            (None, None)
        );
    }

    #[test]
    fn test_summary_covers_all_pages_and_excludes_incomplete_and_unknown_values() {
        let s = mem_storage();
        let empty = s.history_summary(None, None).unwrap();
        assert_eq!(empty.total, 0);
        assert!(empty.avg_download_mbps.is_none());
        for i in 0..125 {
            let mut result = sample_result("completed");
            result.download_mbps = i as f64;
            result.upload_mbps = if i == 0 { Some(0.0) } else { None };
            s.insert_result(&result).unwrap();
        }
        s.insert_result(&sample_result("failed")).unwrap();
        s.insert_result(&sample_result("cancelled")).unwrap();
        let mut outside = sample_result("completed");
        outside.started_at = "2026-11-08 00:00:00".into();
        outside.download_mbps = 9999.0;
        s.insert_result(&outside).unwrap();
        let summary = s
            .history_summary(Some("2026-11-07".into()), Some("2026-11-07".into()))
            .unwrap();
        assert_eq!(summary.total, 127);
        assert_eq!(summary.completed, 125);
        assert_eq!((summary.failed, summary.cancelled), (1, 1));
        assert_eq!(summary.avg_download_mbps, Some(62.0));
        assert_eq!(summary.best_download_mbps, Some(124.0));
        assert_eq!(
            summary.avg_upload_mbps,
            Some(0.0),
            "真实零值保留，未知值跳过"
        );
        s.conn
            .lock()
            .unwrap()
            .execute("UPDATE history SET upload_mbps = NULL", [])
            .unwrap();
        assert!(s
            .history_summary(None, None)
            .unwrap()
            .avg_upload_mbps
            .is_none());
    }
    #[test]
    fn metrics_selects_latest_success_and_counts_all_statuses() {
        let s = mem_storage();
        assert!(s.metrics_snapshot().unwrap().latest.is_none());
        s.insert_result(&sample_result("completed")).unwrap();
        s.insert_result(&sample_result("failed")).unwrap();
        s.insert_result(&sample_result("cancelled")).unwrap();
        let metrics = s.metrics_snapshot().unwrap();
        assert_eq!(
            (
                metrics.total,
                metrics.completed,
                metrics.failed,
                metrics.cancelled
            ),
            (3, 1, 1, 1)
        );
        assert_eq!(metrics.latest.unwrap().status, "completed");
        s.clear_history().unwrap();
        assert_eq!(s.metrics_snapshot().unwrap().total, 0);
    }
}

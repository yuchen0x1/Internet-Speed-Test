//! 实时网络监控:后台任务每秒采样网卡流量,环形缓冲保留 1 小时。
//!
//! - 与测速流量严格区分:监控展示的是网卡层真实收发速率(含所有进程)。
//! - 支持设置开关;关闭时任务空转不采样,零额外开销。
//! - 网络状态变化(连接/断开/切换网卡)通过 net://status 事件通知前端。

use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde::Serialize;
use sysinfo::Networks;
use tauri::Emitter;
use tokio::sync::RwLock;

use crate::diagnostics::{current_net_status, NetStatus};
use crate::settings::AppSettings;

/// 环形缓冲容量上限(样本数)。采样间隔可配(200-5000ms),
/// 因此按时间裁剪而非单纯按条数,保证窗口语义恒为"最近 1 小时"。
const BUFFER_CAPACITY: usize = 3600;
/// 缓冲保留时长
const BUFFER_WINDOW_MS: u64 = 60 * 60 * 1000;
/// 网络状态复检间隔(时间节流,与采样间隔解耦)
const STATUS_CHECK_INTERVAL: Duration = Duration::from_secs(3);

#[derive(Debug, Clone, Serialize)]
pub struct IfaceTick {
    pub name: String,
    pub rx_bps: f64,
    pub tx_bps: f64,
    pub rx_total: u64,
    pub tx_total: u64,
    pub up: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct MonitorTick {
    pub ts_ms: u64,
    /// 全机聚合
    pub rx_bps: f64,
    pub tx_bps: f64,
    pub rx_total: u64,
    pub tx_total: u64,
    pub ifaces: Vec<IfaceTick>,
}

/// A deterministic summary for one wall-clock time bucket.
///
/// The ring buffer currently retains one hour of raw samples. Aggregation is
/// deliberately performed from those samples so the raw export remains the
/// source of truth and callers can choose a bucket size without changing the
/// sampling loop.
#[derive(Debug, Clone, Serialize)]
pub struct MonitorBucket {
    pub start_ts_ms: u64,
    pub end_ts_ms: u64,
    pub sample_count: u32,
    pub rx_avg_bps: f64,
    pub rx_p50_bps: f64,
    pub rx_p95_bps: f64,
    pub rx_peak_bps: f64,
    pub tx_avg_bps: f64,
    pub tx_p50_bps: f64,
    pub tx_p95_bps: f64,
    pub tx_peak_bps: f64,
}

pub struct MonitorHandle {
    enabled: AtomicBool,
    paused: AtomicBool,
    buffer: std::sync::Mutex<VecDeque<MonitorTick>>,
    last_status: std::sync::Mutex<Option<String>>,
}

impl MonitorHandle {
    fn new(enabled: bool) -> Self {
        Self {
            enabled: AtomicBool::new(enabled),
            paused: AtomicBool::new(false),
            buffer: std::sync::Mutex::new(VecDeque::with_capacity(64)),
            last_status: std::sync::Mutex::new(None),
        }
    }

    pub fn set_enabled(&self, on: bool) {
        self.enabled.store(on, Ordering::SeqCst);
    }

    pub fn set_paused(&self, p: bool) {
        self.paused.store(p, Ordering::SeqCst);
    }

    pub fn is_paused(&self) -> bool {
        self.paused.load(Ordering::SeqCst)
    }

    fn push(&self, tick: MonitorTick) {
        let mut buf = crate::safe_lock::lock(&self.buffer);
        // 先按时间淘汰超出保留窗口的尾部,再按条数兜底:
        // 采样间隔从 200ms 调到 5000ms 时,仅按条数会让"1 小时窗口"变成几十分钟或数小时。
        let cutoff = tick.ts_ms.saturating_sub(BUFFER_WINDOW_MS);
        while buf.front().is_some_and(|t| t.ts_ms < cutoff) {
            buf.pop_front();
        }
        while buf.len() >= BUFFER_CAPACITY {
            buf.pop_front();
        }
        buf.push_back(tick);
    }

    /// 返回 window_secs 时间窗内的样本(降采样至 ≤256 点)
    pub fn history(&self, window_secs: u32) -> Vec<MonitorTick> {
        let recent = self.history_window(window_secs);
        if recent.len() <= 256 {
            return recent;
        }
        let step = recent.len() / 256 + 1;
        recent.into_iter().step_by(step).collect()
    }

    /// 返回导出用的原始时间窗样本。导出不降采样，最多覆盖环形缓冲保留的 1 小时。
    pub fn export_history(&self, window_secs: u32) -> Vec<MonitorTick> {
        self.history_window(window_secs)
    }

    /// Return aggregate buckets for the current time window. The aggregation
    /// uses the same raw samples as history/export, so a caller can compare a
    /// bucket summary with the underlying CSV/JSON evidence.
    pub fn aggregate(&self, window_secs: u32, bucket_secs: u32) -> Vec<MonitorBucket> {
        aggregate_ticks(&self.history_window(window_secs), bucket_secs)
    }

    fn history_window(&self, window_secs: u32) -> Vec<MonitorTick> {
        let window_secs = window_secs.clamp(10, 3600) as u64;
        let cutoff = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0)
            .saturating_sub(window_secs * 1000);
        let buf = crate::safe_lock::lock(&self.buffer);
        buf.iter().filter(|t| t.ts_ms >= cutoff).cloned().collect()
    }

    pub fn latest(&self) -> Option<MonitorTick> {
        crate::safe_lock::lock(&self.buffer).back().cloned()
    }
}

fn percentile(values: &[f64], p: f64) -> f64 {
    if values.is_empty() {
        return 0.0;
    }
    let mut sorted = values.to_vec();
    sorted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let rank = ((sorted.len() as f64) * p).ceil() as usize;
    sorted[rank.saturating_sub(1).min(sorted.len() - 1)]
}

/// Aggregate raw monitor ticks into epoch-aligned buckets.
///
/// `bucket_secs` is clamped to at least one second for the pure helper. The
/// Tauri command applies the user-facing allow-list (1 minute, 5 minutes or
/// 1 hour) before calling this function.
pub fn aggregate_ticks(ticks: &[MonitorTick], bucket_secs: u32) -> Vec<MonitorBucket> {
    if ticks.is_empty() {
        return Vec::new();
    }
    let bucket_ms = u64::from(bucket_secs.max(1)) * 1000;
    let mut grouped: std::collections::BTreeMap<u64, Vec<&MonitorTick>> =
        std::collections::BTreeMap::new();
    for tick in ticks {
        let start = (tick.ts_ms / bucket_ms) * bucket_ms;
        grouped.entry(start).or_default().push(tick);
    }
    grouped
        .into_iter()
        .map(|(start, items)| {
            let rx: Vec<f64> = items.iter().map(|t| t.rx_bps).collect();
            let tx: Vec<f64> = items.iter().map(|t| t.tx_bps).collect();
            let rx_sum: f64 = rx.iter().sum();
            let tx_sum: f64 = tx.iter().sum();
            MonitorBucket {
                start_ts_ms: start,
                end_ts_ms: start.saturating_add(bucket_ms),
                sample_count: items.len() as u32,
                rx_avg_bps: rx_sum / items.len() as f64,
                rx_p50_bps: percentile(&rx, 0.50),
                rx_p95_bps: percentile(&rx, 0.95),
                rx_peak_bps: rx.iter().copied().fold(0.0, f64::max),
                tx_avg_bps: tx_sum / items.len() as f64,
                tx_p50_bps: percentile(&tx, 0.50),
                tx_p95_bps: percentile(&tx, 0.95),
                tx_peak_bps: tx.iter().copied().fold(0.0, f64::max),
            }
        })
        .collect()
}

/// 启动后台监控任务(App 生命周期内常驻,受设置开关控制)
pub fn spawn(app: tauri::AppHandle, settings: Arc<RwLock<AppSettings>>) -> Arc<MonitorHandle> {
    let enabled = tauri::async_runtime::block_on(async { settings.read().await.monitor_enabled });
    let handle = Arc::new(MonitorHandle::new(enabled));
    let h = handle.clone();
    tauri::async_runtime::spawn(async move {
        let mut networks = Networks::new_with_refreshed_list().ok();
        let mut last_status_check = std::time::Instant::now();
        loop {
            // 采样间隔取自设置(200-5000ms):每轮重读,改设置后下一轮即生效。
            // netdev 的速率按内部实际经过时间计算,间隔变化不会让 bps 失真。
            let interval_ms = {
                let s = settings.read().await;
                s.refresh_interval_ms.clamp(200, 5000) as u64
            };
            tokio::time::sleep(Duration::from_millis(interval_ms)).await;
            let enabled = h.enabled.load(Ordering::SeqCst);
            let paused = h.paused.load(Ordering::SeqCst);
            if !enabled || paused {
                // 关闭/暂停时空转,不采样不 emit
                continue;
            }
            let Some(nets) = networks.as_mut() else {
                continue;
            };
            // 两次 refresh 之间计算增量速率
            nets.refresh(true);
            let now_ms = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|d| d.as_millis() as u64)
                .unwrap_or(0);
            let mut tick = MonitorTick {
                ts_ms: now_ms,
                rx_bps: 0.0,
                tx_bps: 0.0,
                rx_total: 0,
                tx_total: 0,
                ifaces: Vec::new(),
            };
            for (name, data) in nets.iter() {
                let rx_bps = data.received() as f64;
                let tx_bps = data.transmitted() as f64;
                let up = rx_bps + tx_bps > 0.0;
                tick.rx_bps += rx_bps;
                tick.tx_bps += tx_bps;
                tick.rx_total += data.total_received();
                tick.tx_total += data.total_transmitted();
                tick.ifaces.push(IfaceTick {
                    name: name.to_string_lossy().into_owned(),
                    rx_bps,
                    tx_bps,
                    rx_total: data.total_received(),
                    tx_total: data.total_transmitted(),
                    up,
                });
            }
            tick.ifaces.sort_by(|a, b| {
                b.rx_bps
                    .partial_cmp(&a.rx_bps)
                    .unwrap_or(std::cmp::Ordering::Equal)
            });
            h.push(tick.clone());
            let _ = app.emit("monitor://tick", &tick);

            // 网络状态变化检测:按时间节流(固定 3s),
            // 使状态复检频率不随采样间隔设置漂移
            let now_check = std::time::Instant::now();
            if now_check.duration_since(last_status_check) >= STATUS_CHECK_INTERVAL {
                last_status_check = now_check;
                let status = tokio::task::spawn_blocking(current_net_status)
                    .await
                    .unwrap_or(NetStatus {
                        connected: false,
                        kind: "none".into(),
                        iface: String::new(),
                        link_mbps: None,
                        ipv4: None,
                        has_ipv6: false,
                    });
                let serialized = serde_json::to_string(&status).unwrap_or_default();
                // 在锁内完成比较与更新,不得跨 await 持有 std 锁
                let notify_connected = {
                    let mut last = crate::safe_lock::lock(&h.last_status);
                    if last.as_deref() != Some(serialized.as_str()) {
                        let first_observation = last.is_none();
                        *last = Some(serialized);
                        let _ = app.emit("net://status", &status);
                        if first_observation {
                            None
                        } else {
                            Some(status.connected)
                        }
                    } else {
                        None
                    }
                };
                if let Some(connected) = notify_connected {
                    let s = settings.read().await;
                    crate::scheduler::notify_net_change(&app, &s, connected);
                }
            }
        }
    });
    handle
}

#[cfg(test)]
mod tests {
    use super::{aggregate_ticks, IfaceTick, MonitorTick};

    fn tick(ts_ms: u64, rx_bps: f64, tx_bps: f64) -> MonitorTick {
        MonitorTick {
            ts_ms,
            rx_bps,
            tx_bps,
            rx_total: 0,
            tx_total: 0,
            ifaces: vec![IfaceTick {
                name: "fixture".into(),
                rx_bps,
                tx_bps,
                rx_total: 0,
                tx_total: 0,
                up: true,
            }],
        }
    }

    #[test]
    fn aggregates_epoch_aligned_buckets_with_percentiles_and_peak() {
        let rows = aggregate_ticks(
            &[
                tick(60_001, 10.0, 4.0),
                tick(60_999, 30.0, 8.0),
                tick(120_001, 20.0, 6.0),
            ],
            60,
        );
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].start_ts_ms, 60_000);
        assert_eq!(rows[0].end_ts_ms, 120_000);
        assert_eq!(rows[0].sample_count, 2);
        assert_eq!(rows[0].rx_avg_bps, 20.0);
        assert_eq!(rows[0].rx_p50_bps, 10.0);
        assert_eq!(rows[0].rx_p95_bps, 30.0);
        assert_eq!(rows[0].rx_peak_bps, 30.0);
        assert_eq!(rows[1].tx_avg_bps, 6.0);
    }

    #[test]
    fn empty_and_zero_bucket_inputs_are_safe() {
        assert!(aggregate_ticks(&[], 60).is_empty());
        let rows = aggregate_ticks(&[tick(1, 1.0, 2.0)], 0);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].sample_count, 1);
    }
}

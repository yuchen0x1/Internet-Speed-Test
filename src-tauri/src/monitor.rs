//! 实时网络监控:后台任务每秒采样网卡流量,环形缓冲保留 1 小时。
//!
//! - 与测速流量严格区分:监控展示的是网卡层真实收发速率(含所有进程)。
//! - 支持设置开关;关闭时任务空转不采样,零额外开销。
//! - 网络状态变化(连接/断开/切换网卡)通过 net://status 事件通知前端。

use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde::Serialize;
use sysinfo::Networks;
use tauri::Emitter;
use tokio::sync::RwLock;

use crate::diagnostics::{current_net_status, NetStatus};
use crate::settings::AppSettings;

/// 环形缓冲容量:1s 间隔 × 3600 = 1 小时
const BUFFER_CAPACITY: usize = 3600;
/// 网络状态复检间隔(采样次数)
const STATUS_CHECK_EVERY: u32 = 3;

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

pub struct MonitorHandle {
    enabled: AtomicBool,
    paused: AtomicBool,
    tick_count: AtomicU32,
    buffer: std::sync::Mutex<VecDeque<MonitorTick>>,
    last_status: std::sync::Mutex<Option<String>>,
}

impl MonitorHandle {
    fn new(enabled: bool) -> Self {
        Self {
            enabled: AtomicBool::new(enabled),
            paused: AtomicBool::new(false),
            tick_count: AtomicU32::new(0),
            buffer: std::sync::Mutex::new(VecDeque::with_capacity(BUFFER_CAPACITY + 64)),
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
        let mut buf = self.buffer.lock().unwrap();
        if buf.len() >= BUFFER_CAPACITY {
            buf.pop_front();
        }
        buf.push_back(tick);
    }

    /// 返回 window_secs 时间窗内的样本(降采样至 ≤256 点)
    pub fn history(&self, window_secs: u32) -> Vec<MonitorTick> {
        let window_secs = window_secs.clamp(10, 3600) as u64;
        let cutoff = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0)
            .saturating_sub(window_secs * 1000);
        let buf = self.buffer.lock().unwrap();
        let recent: Vec<MonitorTick> = buf
            .iter()
            .filter(|t| t.ts_ms >= cutoff)
            .cloned()
            .collect();
        if recent.len() <= 256 {
            return recent;
        }
        let step = recent.len() / 256 + 1;
        recent.into_iter().step_by(step).collect()
    }

    pub fn latest(&self) -> Option<MonitorTick> {
        self.buffer.lock().unwrap().back().cloned()
    }
}

/// 启动后台监控任务(App 生命周期内常驻,受设置开关控制)
pub fn spawn(app: tauri::AppHandle, settings: Arc<RwLock<AppSettings>>) -> Arc<MonitorHandle> {
    let enabled = tauri::async_runtime::block_on(async {
        settings.read().await.monitor_enabled
    });
    let handle = Arc::new(MonitorHandle::new(enabled));
    let h = handle.clone();
    tauri::async_runtime::spawn(async move {
        let mut networks = Networks::new_with_refreshed_list().ok();
        let mut interval = tokio::time::interval(Duration::from_millis(1000));
        loop {
            interval.tick().await;
            let enabled = h.enabled.load(Ordering::SeqCst);
            let paused = h.paused.load(Ordering::SeqCst);
            if !enabled || paused {
                // 关闭/暂停时空转,不采样不 emit
                continue;
            }
            let Some(nets) = networks.as_mut() else { continue };
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

            // 网络状态变化检测
            let n = h.tick_count.fetch_add(1, Ordering::SeqCst) + 1;
            if n % STATUS_CHECK_EVERY == 0 {
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
                    let mut last = h.last_status.lock().unwrap();
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

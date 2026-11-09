//! 计划测速:按设定间隔在后台自动执行真实测速,并通过桌面通知汇报结果。
//!
//! - 状态持久化于数据目录 auto_test_state.json,应用重启后节奏不重置
//! - 手动测速进行中时自动跳过本轮(下个 tick 再试)
//! - 结果通知包含下载/上传/节点;失败如实通知原因

use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;

use crate::engine::EventSink;
use crate::settings::AppSettings;
use crate::storage::Storage;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct AutoTestState {
    /// 上次计划测速的启动时间(ms)
    last_started_ms: u64,
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

fn state_path(data_dir: &std::path::Path) -> PathBuf {
    data_dir.join("auto_test_state.json")
}

fn load_state(p: &std::path::Path) -> AutoTestState {
    std::fs::read_to_string(p)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

fn save_state(p: &std::path::Path, s: &AutoTestState) {
    if let Ok(json) = serde_json::to_string(s) {
        let _ = std::fs::write(p, json);
    }
}

fn notify(app: &tauri::AppHandle, settings: &AppSettings, title: &str, body: &str) {
    if !settings.notify_events {
        return;
    }
    use tauri_plugin_notification::NotificationExt as _;
    let _ = app
        .notification()
        .builder()
        .title(title)
        .body(body)
        .show();
}

/// 启动调度器(App 生命周期常驻)
pub fn spawn(
    app: tauri::AppHandle,
    settings: Arc<RwLock<AppSettings>>,
    engine: Arc<crate::engine::SpeedTestEngine>,
    storage: Arc<Storage>,
    data_dir: PathBuf,
) {
    let path = state_path(&data_dir);
    tauri::async_runtime::spawn(async move {
        let mut last_state = load_state(&path);
        let mut interval = tokio::time::interval(Duration::from_secs(1));
        loop {
            interval.tick().await;
            let (enabled, interval_min) = {
                let s = settings.read().await;
                (s.auto_test_enabled, s.auto_test_interval_minutes)
            };
            if !enabled || engine.is_running() {
                continue;
            }
            let due = last_state.last_started_ms == 0
                || now_ms() >= last_state.last_started_ms + (interval_min as u64) * 60_000;
            if !due {
                continue;
            }
            // 记录启动时间(先落盘,避免失败后高频重试)
            last_state.last_started_ms = now_ms();
            save_state(&path, &last_state);

            let result_rx = {
                let s = settings.read().await.clone();
                engine.spawn_test(EventSink::App(app.clone()), s, storage.clone())
            };
            let Ok(result_rx) = result_rx else {
                continue;
            };
            // 等待本次测速结束(轮询运行标志),随后用本次真实结果发通知
            // (不读历史库:autosave 关闭时历史最新一条是上一次的旧结果)
            while engine.is_running() {
                tokio::time::sleep(Duration::from_millis(500)).await;
            }
            let Ok(result) = result_rx.await else {
                continue;
            };
            let settings_snapshot = settings.read().await.clone();
            match result.status.as_str() {
                "completed" => {
                    notify(
                        &app,
                        &settings_snapshot,
                        "计划测速完成",
                        &format!(
                            "下载 {:.1} Mbps / 上传 {} · {}",
                            result.download_mbps,
                            result
                                .upload_mbps
                                .map(|u| format!("{:.1} Mbps", u))
                                .unwrap_or_else(|| "未获得".into()),
                            result
                                .server
                                .as_ref()
                                .map(|s| s.name.clone())
                                .filter(|n| !n.is_empty())
                                .unwrap_or_else(|| "节点未知".into())
                        ),
                    );
                }
                "failed" => {
                    notify(
                        &app,
                        &settings_snapshot,
                        "计划测速失败",
                        result
                            .error
                            .as_deref()
                            .unwrap_or("本次自动测速未能完成,请打开应用查看详情。"),
                    );
                }
                _ => { /* 取消:静默 */ }
            }
        }
    });
}

/// 网络状态变化通知(断开/恢复),由监控任务调用
pub fn notify_net_change(app: &tauri::AppHandle, settings: &AppSettings, connected: bool) {
    if connected {
        notify(app, settings, "网络已恢复", "网络连接已恢复,监控继续运行。");
    } else {
        notify(app, settings, "网络连接已断开", "检测到默认网络不可用,请检查网线/Wi-Fi/路由器。");
    }
}

/// 延迟/丢包阈值告警:每 10s 探测一包,滑动窗口判定,同类告警 10 分钟冷却
pub fn spawn_alerts(
    app: tauri::AppHandle,
    settings: Arc<RwLock<AppSettings>>,
    engine: Arc<crate::engine::SpeedTestEngine>,
) {
    tauri::async_runtime::spawn(async move {
        let mut window: Vec<Option<f64>> = Vec::new();
        let mut last_latency_alert: Option<std::time::Instant> = None;
        let mut last_loss_alert: Option<std::time::Instant> = None;
        let mut last_target: Option<String> = None;
        let mut tick = tokio::time::interval(std::time::Duration::from_secs(10));
        loop {
            tick.tick().await;
            let (enabled, threshold, target) = {
                let s = settings.read().await;
                (s.alert_enabled, s.alert_threshold_ms, s.alert_target.clone())
            };
            if !enabled || engine.is_running() {
                window.clear();
                continue;
            }
            if last_target.as_deref() != Some(target.as_str()) {
                window.clear();
                last_target = Some(target.clone());
            }
            if !crate::diagnostics::valid_host(&target) {
                continue;
            }
            let p = crate::diagnostics::icmp_ping(&target, 1, 1200).await;
            let rtt = p.rtts_ms.first().copied();
            window.push(rtt);
            if window.len() > 30 {
                window.remove(0);
            }
            if window.len() < 5 {
                continue;
            }
            let now = std::time::Instant::now();
            let s = settings.read().await;
            // 丢包告警:最近 3 包全部失败
            if window.len() >= 3 && window.iter().rev().take(3).all(|r| r.is_none()) {
                let cooled = last_loss_alert.map_or(true, |t| now.duration_since(t).as_secs() >= 600);
                if cooled {
                    last_loss_alert = Some(now);
                    notify(
                        &app, &s, "网络丢包告警",
                        &format!("连续 3 次探测 {target} 均超时,请检查网络连通性。"),
                    );
                }
                continue;
            }
            // 延迟告警:最近 5 个成功样本均值超阈值
            let recent_ok: Vec<f64> = window.iter().rev().filter_map(|r| *r).take(5).collect();
            if recent_ok.len() >= 3 {
                let avg = recent_ok.iter().sum::<f64>() / recent_ok.len() as f64;
                if avg > threshold as f64 {
                    let cooled = last_latency_alert.map_or(true, |t| now.duration_since(t).as_secs() >= 600);
                    if cooled {
                        last_latency_alert = Some(now);
                        notify(
                            &app, &s, "延迟过高告警",
                            &format!("对 {target} 的平均延迟 {:.0} ms 已超过阈值 {} ms。", avg, threshold),
                        );
                    }
                }
            }
        }
    });
}

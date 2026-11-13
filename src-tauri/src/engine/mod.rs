//! SpeedTestEngine:测速任务编排器。
//!
//! 职责:
//! - 生命周期管理(防止重复启动、支持安全取消)
//! - 阶段编排:准备 → 延迟 → 下载 → 上传 → 质量 → 完成
//! - 实时采样与事件回传(200ms 速度采样 / 延迟采样 / 负载延迟)
//! - 稳定窗口计算(剔除预热期)、峰值(1s 滑动窗口)
//! - 网络中断检测(计数器停滞看门狗)
//!
//! 单位约定:内部一律使用 字节/秒,仅在展示/入库边界转换为 Mbps。
//! 事件回传通过 [`EventSink`] 抽象:桌面端走 Tauri 事件,CLI 走通道,互不耦合。

pub mod adapter;
pub mod cloudflare;
pub mod librespeed;
pub mod model;
pub mod ndt7;
pub mod quality;
pub mod selector;
pub mod upload_stream;
pub mod util;

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use serde::Serialize;
use tokio_util::sync::CancellationToken;

use crate::diagnostics;
use crate::settings::AppSettings;
use crate::storage::Storage;
use adapter::ThroughputCounters;
use model::{
    bytes_per_sec_to_mbps, EngineConfig, EngineEvent, ServerNodeInfo, TestPhase, TestResult,
    PROVIDER_CUSTOM,
};
use selector::ServerRegistry;

/// 预热期:剔除 TCP 慢启动与 TLS 握手对均值的影响
pub const WARMUP_MS: f64 = 1_200.0;
/// 看门狗:计数器连续停滞 4s 且存在请求失败 → 判定网络中断
const STALL_SAMPLES: u32 = 20;
const SAMPLE_INTERVAL_MS: u64 = 200;

/// 事件出口抽象:引擎与 UI/CLI 解耦
#[derive(Clone)]
pub enum EventSink {
    /// 桌面端:Tauri 事件总线
    App(tauri::AppHandle),
    /// CLI:标准通道(供外部线程实时读取)
    Chan(std::sync::mpsc::Sender<EngineEvent>),
}

impl EventSink {
    pub fn emit(&self, ev: EngineEvent) {
        match self {
            EventSink::App(h) => {
                use tauri::Emitter as _;
                let _ = h.emit("speedtest://event", &ev);
            }
            EventSink::Chan(tx) => {
                let _ = tx.send(ev);
            }
        }
    }

    /// 非引擎主通道的附加事件(仅桌面端有意义)
    pub fn emit_raw(&self, event: &str, payload: impl Serialize + Clone) {
        if let EventSink::App(h) = self {
            use tauri::Emitter as _;
            let _ = h.emit(event, payload);
        }
    }
}

pub struct SpeedTestEngine {
    registry: Arc<ServerRegistry>,
    running: AtomicBool,
    cancel_token: Mutex<Option<CancellationToken>>,
    phase: Mutex<TestPhase>,
}

impl Default for SpeedTestEngine {
    fn default() -> Self {
        Self::new()
    }
}

impl SpeedTestEngine {
    pub fn new() -> Self {
        Self {
            registry: Arc::new(ServerRegistry::new()),
            running: AtomicBool::new(false),
            cancel_token: Mutex::new(None),
            phase: Mutex::new(TestPhase::Idle),
        }
    }

    pub fn registry(&self) -> Arc<ServerRegistry> {
        self.registry.clone()
    }

    pub fn is_running(&self) -> bool {
        self.running.load(Ordering::SeqCst)
    }

    pub fn current_phase(&self) -> TestPhase {
        *crate::safe_lock::lock(&self.phase)
    }

    /// 启动测速(防重复;返回 Err 表示未启动)。事件经 sink 回传,结果如实入库。
    pub fn start(
        self: &Arc<Self>,
        sink: EventSink,
        settings: AppSettings,
        storage: Arc<Storage>,
    ) -> Result<(), String> {
        self.spawn_test(sink, settings, storage).map(|_| ())
    }

    /// 启动测速并返回本次结果的接收器;需要真实结果的调用方(如计划测速通知)使用。
    pub fn spawn_test(
        self: &Arc<Self>,
        sink: EventSink,
        settings: AppSettings,
        storage: Arc<Storage>,
    ) -> Result<tokio::sync::oneshot::Receiver<TestResult>, String> {
        let Some(cancel) = self.try_activate() else {
            return Err("测速任务正在进行中".into());
        };
        let (tx, rx) = tokio::sync::oneshot::channel();
        let engine = self.clone();
        tokio::spawn(async move {
            let result = engine.run_test(&sink, &settings, &storage, &cancel).await;
            // 收尾:释放 token 与运行状态,确保可再次启动
            *crate::safe_lock::lock(&engine.cancel_token) = None;
            *crate::safe_lock::lock(&engine.phase) = TestPhase::Idle;
            engine.running.store(false, Ordering::SeqCst);
            let _ = tx.send(result);
        });
        Ok(rx)
    }

    /// CAS 与 token 写入在同一把锁内完成:stop() 不会读到启动窗口内的 None 而丢失取消。
    /// 独立成方法以便单测(在测试中引用 run_test 会把 tauri GUI 运行时链进测试二进制,
    /// 未带 comctl v6 manifest 的测试 exe 在 System32 comctl32 缺 TaskDialogIndirect
    /// 导出的系统上会以 0xC0000139 无法启动)。
    fn try_activate(&self) -> Option<CancellationToken> {
        let mut slot = crate::safe_lock::lock(&self.cancel_token);
        if self
            .running
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .is_err()
        {
            return None;
        }
        let cancel = CancellationToken::new();
        *slot = Some(cancel.clone());
        Some(cancel)
    }

    /// 一次性运行(供 CLI):阻塞至完成,返回完整结果,不经过事件循环。
    pub async fn run_once(
        &self,
        sink: EventSink,
        settings: AppSettings,
        storage: Arc<Storage>,
    ) -> TestResult {
        let token = CancellationToken::new();
        self.run_test(&sink, &settings, &storage, &token).await
    }

    /// 安全取消:通知所有阶段终止,由 run_test 收尾并释放资源
    pub fn stop(&self) {
        if let Some(t) = crate::safe_lock::lock(&self.cancel_token).as_ref() {
            t.cancel();
        }
    }

    fn set_phase(&self, sink: &EventSink, p: TestPhase, message: &str) {
        *crate::safe_lock::lock(&self.phase) = p;
        sink.emit(EngineEvent::Phase {
            phase: p.as_str().to_string(),
            message: message.into(),
        });
    }

    /// 完整测速流程,返回最终结果(已完成入库,如设置允许)
    async fn run_test(
        &self,
        sink: &EventSink,
        settings: &AppSettings,
        storage: &Arc<Storage>,
        token: &CancellationToken,
    ) -> TestResult {
        let t0 = Instant::now();
        let mut result = TestResult {
            id: util::short_id(),
            started_at: chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string(),
            ..Default::default()
        };
        let cfg = EngineConfig {
            duration_secs: settings.test_duration_secs.clamp(5, 30) as u64,
            down_streams: settings.down_concurrency.clamp(1, 16) as usize,
            up_streams: settings.up_concurrency.clamp(1, 8) as usize,
            precision: settings.precision_mode,
            auto_select: settings.node_mode == "auto",
            manual_node_url: if settings.node_mode == "auto" {
                None
            } else {
                Some(settings.default_node_url.clone())
            },
        };
        result.conditions.push(format!(
            "{}s×{} 连接下载 / {}s×{} 连接上传",
            cfg.duration_secs, cfg.down_streams, cfg.duration_secs, cfg.up_streams
        ));
        if cfg.precision {
            result
                .conditions
                .push("精度模式:样本密度与并发已提高".into());
        }

        let outcome = self
            .execute(sink, settings, storage, token, &mut result, &cfg, t0)
            .await;

        match outcome {
            Ok(true) => {
                result.status = "completed".into();
                result.quality = Some(quality::analyze_quality(&result));
            }
            Ok(false) => {
                result.status = "cancelled".into();
                // 未完成的阶段不能据缺失/默认指标给出完整质量评级。
                result.quality = None;
            }
            Err(err) => {
                result.status = "failed".into();
                result.error = Some(err);
                // 失败时丢弃可重新发现的节点缓存:NDT7 社区节点可能已下线,
                // 下次测速应重新走 Locate 而不是复用同一个死节点。
                self.registry.invalidate_discovery();
            }
        }
        result.finished_at = chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string();
        result.duration_ms = t0.elapsed().as_millis() as u64;
        // 所有终态先等待写库，再通知 UI 刷新；失败/取消也携带本次完整结果。
        // 保存失败仍展示真实测量值，并单独提示，不能伪装成已保存。
        if settings.autosave_history {
            if let Err(e) = crate::storage::insert_result_blocking(storage, &result).await {
                log::warn!("历史记录写入失败: {e}");
                result.history_save_error =
                    Some("历史记录保存失败，请检查磁盘空间或数据目录权限".into());
            }
        }
        match result.status.as_str() {
            "completed" => self.set_phase(sink, TestPhase::Finished, "测速完成"),
            "cancelled" => sink.emit(EngineEvent::Cancelled),
            _ => sink.emit(EngineEvent::Failed {
                error: result.error.clone().unwrap_or_else(|| "测速失败".into()),
            }),
        }
        sink.emit(EngineEvent::Finished {
            result: Box::new(result.clone()),
        });
        result
    }

    /// 流程主体:Ok(true)=完成,Ok(false)=取消,Err=失败
    #[allow(clippy::too_many_arguments)]
    async fn execute(
        &self,
        sink: &EventSink,
        settings: &AppSettings,
        storage: &Arc<Storage>,
        token: &CancellationToken,
        result: &mut TestResult,
        cfg: &EngineConfig,
        t0: Instant,
    ) -> Result<bool, String> {
        // ---------- 阶段一:准备 ----------
        self.set_phase(sink, TestPhase::Preparing, "正在检查网络连接与本地接口");
        let ifaces = diagnostics::collect_interfaces();
        if let Some(def) = ifaces.iter().find(|i| i.is_default) {
            result.network_type = def.kind.clone();
            result.interface_name = def
                .friendly_name
                .clone()
                .unwrap_or_else(|| def.name.clone());
        } else {
            return Err("未检测到可用的本地网络接口".into());
        }

        // ---------- 节点选择 ----------
        self.set_phase(sink, TestPhase::Preparing, "正在获取测速节点列表");
        let favorites = storage.favorites_map();
        let nodes = self
            .registry
            .list_servers(&settings.custom_nodes, &favorites)
            .await;
        if token.is_cancelled() {
            return Ok(false);
        }

        let node: ServerNodeInfo = if cfg.auto_select {
            self.set_phase(sink, TestPhase::Preparing, "正在探测节点并自动优选");
            let (chosen, probes) = self.registry.auto_select(&nodes, token).await;
            sink.emit_raw(
                "servers://probed",
                probes
                    .iter()
                    .map(|p| {
                        serde_json::json!({
                            "id": p.node.id, "alive": p.alive, "latencyMs": p.latency_ms
                        })
                    })
                    .collect::<Vec<_>>(),
            );
            match chosen {
                Some(n) => {
                    log::info!("已自动选择测速节点(优选完成)");
                    n
                }
                None => {
                    return Err("未找到可用的测速节点:所有节点探测失败,请检查网络或稍后重试".into())
                }
            }
        } else {
            let target = cfg
                .manual_node_url
                .as_deref()
                .unwrap_or("")
                .trim_end_matches('/')
                .to_string();
            match nodes.iter().find(|n| n.base_url == target) {
                Some(n) => n.clone(),
                None => match manual_node_fallback(&target) {
                    Some(n) => {
                        log::warn!("手动节点不在列表中,按 LibreSpeed 默认端点构造(由真实测试验证)");
                        n
                    }
                    None => return Err("所选测速节点不可用,请在节点管理中重新选择".into()),
                },
            }
        };
        if token.is_cancelled() {
            return Ok(false);
        }
        result.server = Some(node.clone());
        sink.emit(EngineEvent::Server { node: node.clone() });

        // 连通性确认
        self.registry
            .ping_node(&node, token)
            .await
            .map_err(|e| format!("测速节点连通性检查失败: {e}"))?;

        // 公网 IP(仅当用户授权显示时获取)
        if settings.show_public_ip {
            if let Some((ip, isp)) = self.registry.node_ip_info(&node, token).await {
                result.public_ip = Some(ip);
                result.isp = isp;
            }
        }

        // ---------- 阶段二:延迟测试 ----------
        self.set_phase(sink, TestPhase::Latency, "正在测量网络延迟");
        let ping_count = if cfg.precision { 16 } else { 10 };
        let mut rtts: Vec<f64> = Vec::with_capacity(ping_count);
        for i in 0..ping_count {
            if token.is_cancelled() {
                return Ok(false);
            }
            sink.emit(EngineEvent::Progress {
                percent: (i as f32 / ping_count as f32) * 100.0,
            });
            match self.registry.ping_node(&node, token).await {
                Ok(rtt) => {
                    rtts.push(rtt);
                    result
                        .latency_samples
                        .push([t0.elapsed().as_millis() as f64, rtt]);
                    sink.emit(EngineEvent::Latency { rtt_ms: rtt });
                }
                Err(_) => { /* 单次失败跳过 */ }
            }
            tokio::time::sleep(Duration::from_millis(350)).await;
        }
        if rtts.is_empty() {
            return Err("延迟测试失败:所有测量请求均超时或被拒绝".into());
        }
        let mut sorted_rtts = rtts.clone();
        result.idle_ping_ms = quality::median(&mut sorted_rtts);
        result.ping_min_ms = rtts.iter().cloned().fold(f64::MAX, f64::min);
        result.ping_max_ms = rtts.iter().cloned().fold(0.0, f64::max);

        // ---------- 阶段三:下载测试 ----------
        self.set_phase(sink, TestPhase::Download, "正在执行下载测速");
        let dl = self
            .run_throughput_phase(
                sink,
                &node,
                "download",
                cfg.down_streams,
                cfg.duration_secs,
                cfg.precision,
                token,
                &mut result.down_samples,
            )
            .await;
        match dl {
            PhaseOutcome::Ok(bytes, failures, loaded, warmup_bytes) => {
                result.http_failures += failures;
                result.loaded_down_ms = loaded;
                result.download_mbps = quality::stable_window_mbps(
                    bytes,
                    cfg.duration_secs as f64 * 1000.0,
                    WARMUP_MS,
                    warmup_bytes,
                );
                result.download_peak_mbps = quality::peak_from_samples(&result.down_samples);
                if bytes == 0 {
                    return Err(
                        "下载测试失败:未能从测速服务器接收到数据(服务器不可用或网络中断)".into(),
                    );
                }
            }
            PhaseOutcome::Cancelled => return Ok(false),
            PhaseOutcome::Failed(e) => return Err(e),
        }

        // ---------- 阶段四:上传测试 ----------
        self.set_phase(sink, TestPhase::Upload, "正在执行上传测速");
        let ul = self
            .run_throughput_phase(
                sink,
                &node,
                "upload",
                cfg.up_streams,
                cfg.duration_secs,
                cfg.precision,
                token,
                &mut result.up_samples,
            )
            .await;
        match ul {
            PhaseOutcome::Ok(bytes, failures, loaded, warmup_bytes) => {
                result.http_failures += failures;
                result.loaded_up_ms = loaded;
                if bytes == 0 {
                    // 上传失败不终止整体测试,如实标注
                    result.upload_mbps = None;
                    result
                        .conditions
                        .push("上传测试未成功:服务器拒绝了全部上传请求".into());
                } else {
                    result.upload_mbps = Some(quality::stable_window_mbps(
                        bytes,
                        cfg.duration_secs as f64 * 1000.0,
                        WARMUP_MS,
                        warmup_bytes,
                    ));
                    result.upload_peak_mbps = Some(quality::peak_from_samples(&result.up_samples));
                }
            }
            PhaseOutcome::Cancelled => return Ok(false),
            PhaseOutcome::Failed(e) => {
                result.conditions.push(format!("上传阶段异常: {e}"));
            }
        }

        // ---------- 阶段五:网络质量测试 ----------
        self.set_phase(sink, TestPhase::Quality, "正在分析网络质量(丢包 / 抖动)");
        let all_rtts: Vec<f64> = result.latency_samples.iter().map(|s| s[1]).collect();
        result.jitter_ms = quality::jitter_ms(&all_rtts);
        // ICMP 丢包:对测速服务器主机真实 ping(10 包);被禁 ping 则如实标注
        if let Some(server) = &result.server {
            if let Some(host) = util::extract_host(&server.base_url) {
                let ping_out = diagnostics::icmp_ping(&host, 10, 1500).await;
                result.packet_loss_percent = ping_out.loss_percent;
            }
        }

        Ok(true)
    }

    /// 执行一个吞吐阶段(下载或上传),管理采样器 / 负载延迟探测 / 看门狗
    #[allow(clippy::too_many_arguments)]
    async fn run_throughput_phase(
        &self,
        sink: &EventSink,
        node: &ServerNodeInfo,
        direction: &str,
        streams: usize,
        duration_secs: u64,
        precision: bool,
        parent: &CancellationToken,
        samples_out: &mut Vec<[f64; 2]>,
    ) -> PhaseOutcome {
        let child = parent.child_token();
        let counters = Arc::new(ThroughputCounters::default());
        let samples = Arc::new(Mutex::new(Vec::<[f64; 2]>::new()));
        let bytes_log = Arc::new(Mutex::new(Vec::<(u64, u64)>::new()));
        let loaded = Arc::new(Mutex::new(Vec::<f64>::new()));
        let stalled = Arc::new(AtomicBool::new(false));
        let duration = Duration::from_secs(duration_secs);

        // 实时采样器 + 停滞看门狗
        let sampler_handle = {
            let counters = counters.clone();
            let samples = samples.clone();
            let bytes_log = bytes_log.clone();
            let stalled = stalled.clone();
            let child = child.clone();
            let sink = sink.clone();
            let direction = direction.to_string();
            tokio::spawn(async move {
                sampler_task(
                    &sink, &counters, &samples, &bytes_log, &child, &stalled, direction,
                )
                .await;
            })
        };

        // 负载延迟探测(每 500ms 一次)
        let loaded_handle = {
            let registry = self.registry.clone();
            let node = node.clone();
            let loaded = loaded.clone();
            let sink = sink.clone();
            let direction = direction.to_string();
            let child = child.clone();
            tokio::spawn(async move {
                loaded_ping_task(&sink, &registry, &node, &loaded, &child, direction).await;
            })
        };

        let adapter = self.registry.adapter_dyn(node);
        let cfg_phase = adapter::PhaseConfig {
            duration,
            streams: if precision { streams + 2 } else { streams },
        };
        // 标注实际执行的协议适配器,便于排查"同一节点换协议后结果差异"
        log::info!(
            "{} 阶段使用 {} 适配器(并发 {})",
            direction,
            adapter.provider(),
            cfg_phase.streams
        );
        if direction == "download" {
            adapter
                .run_download(node, counters.clone(), cfg_phase, child.clone())
                .await;
        } else {
            adapter
                .run_upload(node, counters.clone(), cfg_phase, child.clone())
                .await;
        }
        child.cancel();
        // 等两个后台任务确认退出(替代固定 280ms 等待):样本与负载延迟读取无竞态
        let _ = sampler_handle.await;
        let _ = loaded_handle.await;

        let bytes = counters.total();
        let failures = counters.failure_count();
        samples_out.extend(crate::safe_lock::lock(&samples).drain(..));
        if parent.is_cancelled() {
            return PhaseOutcome::Cancelled;
        }
        if stalled.load(Ordering::SeqCst) && bytes == 0 {
            return PhaseOutcome::Failed("数据传输完全停滞(疑似网络中断或服务器故障)".into());
        }
        // 预热结束时已发送的字节数(从字节日志取第一个 ≥ WARMUP 的点)
        let warmup_bytes = crate::safe_lock::lock(&bytes_log)
            .iter()
            .find(|(ms, _)| *ms as f64 >= WARMUP_MS)
            .map(|(_, b)| *b)
            .unwrap_or(bytes);
        let loaded_median = {
            let mut v = crate::safe_lock::lock(&loaded).clone();
            if v.is_empty() {
                None
            } else {
                Some(quality::median(&mut v))
            }
        };
        PhaseOutcome::Ok(bytes, failures, loaded_median, warmup_bytes)
    }
}

enum PhaseOutcome {
    /// (总字节, HTTP 失败次数, 负载延迟中位数, 预热期字节数)
    Ok(u64, u32, Option<f64>, u64),
    Cancelled,
    Failed(String),
}

/// 手动指定节点不在列表中时的兜底:仅接受合法 HTTPS URL,按 LibreSpeed 默认端点构造。
/// 不猜参数、不降级为 HTTP;能否测速由后续真实请求验证,失败会如实上报。
pub fn manual_node_fallback(url: &str) -> Option<ServerNodeInfo> {
    let base = util::validate_server_url(url, false).ok()?;
    let name = util::extract_host(&base).unwrap_or_else(|| "自定义节点".into());
    Some(ServerNodeInfo {
        id: util::node_id(&base, &name),
        location: Some(name.clone()),
        name,
        provider: PROVIDER_CUSTOM.into(),
        base_url: base,
        ..Default::default()
    })
}

/// 采样器:200ms 计数器差分 → 瞬时速度;停滞看门狗
async fn sampler_task(
    sink: &EventSink,
    counters: &Arc<ThroughputCounters>,
    samples: &Arc<Mutex<Vec<[f64; 2]>>>,
    bytes_log: &Arc<Mutex<Vec<(u64, u64)>>>,
    token: &CancellationToken,
    stalled: &AtomicBool,
    direction: String,
) {
    let mut last_bytes: u64 = 0;
    let mut stall_count: u32 = 0;
    let mut elapsed_ms: u64 = 0;
    loop {
        if token.is_cancelled() {
            return;
        }
        tokio::time::sleep(Duration::from_millis(SAMPLE_INTERVAL_MS)).await;
        if token.is_cancelled() {
            return;
        }
        elapsed_ms += SAMPLE_INTERVAL_MS;
        let cur = counters.total();
        let inst_bps = cur.saturating_sub(last_bytes) as f64 / (SAMPLE_INTERVAL_MS as f64 / 1000.0);
        let mbps = bytes_per_sec_to_mbps(inst_bps);

        crate::safe_lock::lock(samples).push([elapsed_ms as f64, mbps]);
        crate::safe_lock::lock(bytes_log).push((elapsed_ms, cur));
        sink.emit(EngineEvent::Speed {
            direction: direction.clone(),
            mbps,
            cumulative_mb: cur as f64 / 1_000_000.0,
        });

        // 停滞看门狗:连续 4s 字节无增长且存在失败 → 判定中断,提前结束该阶段
        if cur == last_bytes {
            stall_count += 1;
            if stall_count >= STALL_SAMPLES && counters.failure_count() > 0 {
                stalled.store(true, Ordering::SeqCst);
                token.cancel();
                return;
            }
        } else {
            stall_count = 0;
        }
        last_bytes = cur;
    }
}

/// 负载延迟探测:吞吐阶段内每 500ms 测一次到节点的小请求 RTT
async fn loaded_ping_task(
    sink: &EventSink,
    registry: &Arc<ServerRegistry>,
    node: &ServerNodeInfo,
    loaded: &Arc<Mutex<Vec<f64>>>,
    token: &CancellationToken,
    direction: String,
) {
    let mut tick = tokio::time::interval(Duration::from_millis(500));
    tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    loop {
        tokio::select! {
            _ = token.cancelled() => return,
            _ = tick.tick() => {
                if let Ok(rtt) = registry.ping_node(node, token).await {
                    crate::safe_lock::lock(loaded).push(rtt);
                    sink.emit(EngineEvent::LoadedLatency {
                        direction: direction.clone(),
                        rtt_ms: rtt,
                    });
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_manual_node_fallback_https_only() {
        // 合法 HTTPS 节点:按 LibreSpeed 默认端点构造,ID 稳定
        let n = manual_node_fallback("https://cs.lt6.ltd/").expect("应构造成功");
        assert_eq!(n.base_url, "https://cs.lt6.ltd");
        assert_eq!(n.name, "cs.lt6.ltd");
        assert_eq!(n.provider, PROVIDER_CUSTOM);
        assert_eq!(n.dl_url, "garbage.php");
        assert_eq!(n.ul_url, "empty.php");
        assert_eq!(n.ping_url, "empty.php");
        assert!(n
            .download_url()
            .starts_with("https://cs.lt6.ltd/garbage.php"));

        // HTTP / 非 URL / 空值一律拒绝(不静默降级)
        assert!(manual_node_fallback("http://speed.example.com").is_none());
        assert!(manual_node_fallback("ftp://x/").is_none());
        assert!(manual_node_fallback("not a url").is_none());
        assert!(manual_node_fallback("").is_none());
        assert!(manual_node_fallback("javascript:alert(1)").is_none());
    }

    #[test]
    fn test_manual_node_fallback_id_stable() {
        let a = manual_node_fallback("https://a.example/").unwrap();
        let b = manual_node_fallback("https://a.example").unwrap();
        assert_eq!(a.id, b.id, "带/不带尾斜杠应得到同一节点 ID");
    }

    /// 防重复启动:启动 CAS 与 token 写入是原子的(见 try_activate 注释)
    #[test]
    fn test_double_start_rejected() {
        let engine = SpeedTestEngine::new();
        assert!(!engine.is_running());
        assert!(
            engine.try_activate().is_some(),
            "首次激活应成功并写入取消 token"
        );
        assert!(engine.is_running(), "激活后应处于运行状态");
        assert!(engine.try_activate().is_none(), "运行中重复激活必须被拒绝");
        // stop 只取消当前 token;运行标志由 run_test 收尾复位,测试中不触发
        engine.stop();
    }
}

//! Tauri 命令层:前端唯一入口。所有业务在引擎/诊断/存储层完成,本层仅做编排与授权检查。

use std::borrow::Cow;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use serde::Serialize;
use tauri::AppHandle;

use crate::diagnostics;
use crate::engine::model::{ServerNodeInfo, TestResult};
use crate::engine::util;
use crate::monitor::MonitorHandle;
use crate::settings::{AppSettings, SettingsManager};
use crate::storage::{HistoryRow, Storage};

pub struct AppState {
    pub settings: SettingsManager,
    pub storage: Arc<Storage>,
    pub engine: Arc<crate::engine::SpeedTestEngine>,
    pub monitor: Arc<MonitorHandle>,
    pub pingmon: Arc<crate::pingmon::PingMonState>,
    pub batchtest: Arc<crate::batchtest::BatchTestState>,
    pub prometheus: Arc<crate::prometheus::PrometheusHandle>,
    pub data_dir: PathBuf,
}

#[derive(Serialize)]
pub struct AppInfo {
    pub name: String,
    pub version: String,
    pub os: String,
    pub data_dir: String,
    /// 设置文件完整路径(关于页展示,便于用户排查/备份)
    pub settings_path: String,
}

/// 诊断命令统一错误结构:code 供前端分支判断,message 直接展示。
/// 码表:invalid_input(入参为空/格式错) / invalid_host(主机名校验失败) /
/// failed(诊断执行失败,原始文案在 message)
#[derive(Debug, Clone, Serialize)]
pub struct CommandError {
    pub code: String,
    pub message: String,
}

impl CommandError {
    pub fn new(code: &str, message: impl Into<String>) -> Self {
        Self {
            code: code.into(),
            message: message.into(),
        }
    }
}

impl From<String> for CommandError {
    fn from(message: String) -> Self {
        Self::new("failed", message)
    }
}

impl From<&str> for CommandError {
    fn from(message: &str) -> Self {
        Self::new("failed", message)
    }
}

/// 历史导出结果:超出 5000 条时如实告知截断情况
#[derive(Debug, Clone, Serialize)]
pub struct ExportOutcome {
    pub path: String,
    pub exported: u32,
    pub total: u32,
}

#[tauri::command]
pub fn get_app_info(state: tauri::State<AppState>) -> AppInfo {
    AppInfo {
        name: "NetVision Pro".into(),
        version: env!("CARGO_PKG_VERSION").into(),
        os: std::env::consts::OS.to_string(),
        data_dir: state.data_dir.display().to_string(),
        settings_path: state.settings.path().display().to_string(),
    }
}

// ============================================================
// 设置
// ============================================================

#[tauri::command]
pub async fn get_settings(state: tauri::State<'_, AppState>) -> Result<AppSettings, String> {
    Ok(state.settings.settings.read().await.clone())
}

#[tauri::command]
pub async fn set_settings(
    app: AppHandle,
    state: tauri::State<'_, AppState>,
    settings: AppSettings,
) -> Result<AppSettings, String> {
    // 并发约束(引擎层还会二次 clamp)
    let mut s = settings;
    s.down_concurrency = s.down_concurrency.clamp(1, 16);
    s.up_concurrency = s.up_concurrency.clamp(1, 8);
    s.test_duration_secs = s.test_duration_secs.clamp(5, 30);
    s.anim_intensity = s.anim_intensity.clamp(0.0, 1.5);
    s.refresh_interval_ms = s.refresh_interval_ms.clamp(200, 5000);
    s.auto_test_interval_minutes = s.auto_test_interval_minutes.clamp(15, 1440);
    s.update_check_interval_hours = s.update_check_interval_hours.clamp(6, 168);
    if !["dark", "light", "system"].contains(&s.theme.as_str()) {
        s.theme = "dark".into();
    }
    if !["auto", "manual"].contains(&s.node_mode.as_str()) {
        s.node_mode = "auto".into();
    }

    let mut guard = state.settings.settings.write().await;
    // 节点列表仅由专用增删命令维护，旧前端完整快照不能将它覆盖。
    s.custom_nodes = guard.custom_nodes.clone();

    // 开机自启动真实生效
    use tauri_plugin_autostart::ManagerExt as _;
    let autolaunch = app.autolaunch();
    let previous = if s.autostart != guard.autostart {
        Some(autolaunch.is_enabled().map_err(|e| e.to_string())?)
    } else {
        None
    };
    state
        .settings
        .save_with_autostart(&s, previous, |enabled| {
            if enabled {
                autolaunch.enable()
            } else {
                autolaunch.disable()
            }
            .map_err(|e| e.to_string())
        })
        .await?;
    *guard = s.clone();

    state.monitor.set_enabled(s.monitor_enabled);

    // Prometheus 端点按设置启停(apply 内部自动判重)
    {
        let storage = state.storage.clone();
        let connected_getter: Arc<dyn Fn() -> bool + Send + Sync> =
            Arc::new(|| crate::diagnostics::current_net_status().connected);
        state.prometheus.apply(
            s.prometheus_enabled,
            s.prometheus_port,
            storage,
            connected_getter,
        );
    }
    Ok(s)
}

#[tauri::command]
pub fn get_network_status() -> diagnostics::NetStatus {
    diagnostics::current_net_status()
}

/// Prometheus 端点的真实监听状态。
/// 设置里的开关只表示"用户想要开启",端口可能已被占用;
/// 设置页必须依据本命令区分"已开启"和"开启失败"。
#[tauri::command]
pub async fn prometheus_status(
    state: tauri::State<'_, AppState>,
) -> Result<crate::prometheus::PrometheusStatus, String> {
    Ok(state.prometheus.status())
}

// ============================================================
// 测速节点
// ============================================================

#[tauri::command]
pub async fn servers_list(
    state: tauri::State<'_, AppState>,
) -> Result<Vec<ServerNodeInfo>, String> {
    let settings = state.settings.settings.read().await;
    let favorites = state.storage.favorites_map();
    let engine = state.engine.clone();
    let custom = settings.custom_nodes.clone();
    drop(settings);
    Ok(engine.registry().list_servers(&custom, &favorites).await)
}

/// 刷新节点:重新拉列表 + 并发探测全部节点(返回含延迟与可用性)
#[tauri::command]
pub async fn servers_refresh(
    state: tauri::State<'_, AppState>,
) -> Result<Vec<ServerNodeInfo>, String> {
    let settings = state.settings.settings.read().await;
    let custom = settings.custom_nodes.clone();
    drop(settings);
    let favorites = state.storage.favorites_map();
    let registry = state.engine.registry();
    let mut nodes = registry.list_servers(&custom, &favorites).await;
    let token = tokio_util::sync::CancellationToken::new();
    let probes = registry.probe_nodes(&nodes, 8, &token).await;
    let mut map: HashMap<String, (bool, Option<f64>)> = HashMap::new();
    for p in probes {
        map.insert(p.node.id, (p.alive, p.latency_ms));
    }
    for n in &mut nodes {
        if let Some((alive, lat)) = map.get(&n.id) {
            n.alive = Some(*alive);
            n.latency_ms = *lat;
        }
    }
    nodes.sort_by(|a, b| {
        let al = a.latency_ms.unwrap_or(9_999.0);
        let bl = b.latency_ms.unwrap_or(9_999.0);
        al.partial_cmp(&bl).unwrap_or(std::cmp::Ordering::Equal)
    });
    Ok(nodes)
}

#[tauri::command]
pub async fn servers_set_favorite(
    state: tauri::State<'_, AppState>,
    url: String,
    name: String,
    favorite: bool,
) -> Result<(), String> {
    state.storage.set_favorite(&url, &name, favorite)
}

#[tauri::command]
pub async fn servers_set_manual(
    state: tauri::State<'_, AppState>,
    url: String,
) -> Result<(), String> {
    let mut guard = state.settings.settings.write().await;
    let url = crate::settings::validate_manual_node(&guard, &url)?;
    let mut s = guard.clone();
    s.node_mode = "manual".into();
    s.default_node_url = url;
    state.settings.save(&s).await?;
    *guard = s;
    Ok(())
}

#[tauri::command]
pub async fn servers_set_auto(state: tauri::State<'_, AppState>) -> Result<(), String> {
    let mut guard = state.settings.settings.write().await;
    let mut s = guard.clone();
    s.node_mode = "auto".into();
    state.settings.save(&s).await?;
    *guard = s;
    Ok(())
}

#[tauri::command]
pub async fn servers_add_custom(
    state: tauri::State<'_, AppState>,
    node: ServerNodeInfo,
) -> Result<Vec<ServerNodeInfo>, String> {
    let base = util::validate_server_url(&node.base_url, node.allow_http)?;
    let mut n = node;
    n.base_url = base;
    if n.name.trim().is_empty() {
        n.name = util::extract_host(&n.base_url).unwrap_or_else(|| "自定义节点".into());
    }
    n.provider = "custom".into();
    n.id = util::node_id(&n.base_url, &n.name);
    n.alive = None;
    n.latency_ms = None;
    let mut guard = state.settings.settings.write().await;
    let mut s = guard.clone();
    if s.custom_nodes.iter().any(|c| c.base_url == n.base_url) {
        return Err("该节点已存在".into());
    }
    s.custom_nodes.push(n);
    let list = s.custom_nodes.clone();
    state.settings.save(&s).await?;
    *guard = s;
    Ok(list)
}

#[tauri::command]
pub async fn servers_remove_custom(
    state: tauri::State<'_, AppState>,
    url: String,
) -> Result<Vec<ServerNodeInfo>, String> {
    let mut guard = state.settings.settings.write().await;
    let mut s = guard.clone();
    s.custom_nodes.retain(|c| c.base_url != url);
    if s.node_mode == "manual" && s.default_node_url == url {
        s.node_mode = "auto".into();
        s.default_node_url.clear();
    }
    let list = s.custom_nodes.clone();
    state.settings.save(&s).await?;
    *guard = s;
    Ok(list)
}

// ============================================================
// 测速控制
// ============================================================

#[tauri::command]
pub async fn speedtest_start(
    app: AppHandle,
    state: tauri::State<'_, AppState>,
) -> Result<(), String> {
    let settings = state.settings.settings.read().await.clone();
    state.engine.start(
        crate::engine::EventSink::App(app),
        settings,
        state.storage.clone(),
    )
}

#[tauri::command]
pub fn speedtest_stop(state: tauri::State<AppState>) {
    state.engine.stop();
}

#[tauri::command]
pub fn speedtest_running(state: tauri::State<AppState>) -> bool {
    state.engine.is_running()
}

/// 引擎当前阶段(与 speedtest_running 配套用于前端状态对账)。
/// 托盘可直接启动测速,窗口重载也会丢失事件,前端需要能读到真实阶段。
#[tauri::command]
pub fn speedtest_phase(state: tauri::State<AppState>) -> String {
    state.engine.current_phase().as_str().to_string()
}

/// 国内外知名测速服务官网白名单(仅 HTTPS 固定入口;未知服务一律拒绝,不接受任意 URL)
pub fn speedtest_service_url(service: &str) -> Option<&'static str> {
    match service {
        "speedtest_cn" => Some("https://www.speedtest.cn/"),
        "ookla" => Some("https://www.speedtest.net/"),
        "fast" => Some("https://fast.com/"),
        "cloudflare" => Some("https://speed.cloudflare.com/"),
        "telecom_gd" => Some("https://m.gd.189.cn/kdcs/service.html"),
        "unicom" => Some("https://iservice.10010.com/e4/transact/basic/service_broadband.html"),
        "mobile_bj" => Some("https://service.bj.10086.cn/m/mbjyd/kdzz/page1.html"),
        _ => None,
    }
}

/// 仅打开内置白名单中的测速官网；不在桌面端抓取、代理或代替网站执行测试。
#[tauri::command]
pub fn open_speedtest_service(service: String) -> Result<(), String> {
    let url = speedtest_service_url(&service).ok_or_else(|| "不支持的测速服务".to_string())?;
    #[cfg(target_os = "windows")]
    {
        // explorer.exe 是 Windows 的 URL 协议处理入口(等价 ShellExecute),
        // 不经命令行解释器、参数为白名单常量,无注入面。
        std::process::Command::new("explorer.exe")
            .arg(url)
            .spawn()
            .map(|_| ())
            .map_err(|e| format!("无法打开测速网站: {e}"))
    }
    #[cfg(not(target_os = "windows"))]
    {
        let _ = url;
        Err("外部测速网站入口仅支持 Windows 桌面版".into())
    }
}

// ============================================================
// 历史
// ============================================================

#[tauri::command]
pub async fn history_list(
    state: tauri::State<'_, AppState>,
    from: Option<String>,
    to: Option<String>,
    desc: Option<bool>,
    limit: Option<u32>,
    cursor_id: Option<i64>,
) -> Result<Vec<HistoryRow>, String> {
    let storage = state.storage.clone();
    tauri::async_runtime::spawn_blocking(move || {
        storage.list_history_page(
            from,
            to,
            desc.unwrap_or(true),
            limit.unwrap_or(500),
            cursor_id,
        )
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn history_count(
    state: tauri::State<'_, AppState>,
    from: Option<String>,
    to: Option<String>,
) -> Result<u32, String> {
    let storage = state.storage.clone();
    tauri::async_runtime::spawn_blocking(move || storage.history_count_filtered(from, to))
        .await
        .map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn history_detail(
    state: tauri::State<'_, AppState>,
    id: i64,
) -> Result<Option<TestResult>, String> {
    let storage = state.storage.clone();
    tauri::async_runtime::spawn_blocking(move || storage.get_result_detail(id))
        .await
        .map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn history_summary(
    state: tauri::State<'_, AppState>,
    from: Option<String>,
    to: Option<String>,
) -> Result<crate::storage::HistorySummary, String> {
    let storage = state.storage.clone();
    tauri::async_runtime::spawn_blocking(move || storage.history_summary(from, to))
        .await
        .map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn history_delete(state: tauri::State<'_, AppState>, id: i64) -> Result<(), String> {
    let storage = state.storage.clone();
    tauri::async_runtime::spawn_blocking(move || storage.delete_history(id))
        .await
        .map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn history_clear(state: tauri::State<'_, AppState>) -> Result<(), String> {
    let storage = state.storage.clone();
    tauri::async_runtime::spawn_blocking(move || storage.clear_history())
        .await
        .map_err(|e| e.to_string())?
}

/// 打开系统保存对话框并写出 CSV / JSON;超出 5000 条时如实返回截断情况
#[tauri::command]
pub async fn history_export(
    app: AppHandle,
    state: tauri::State<'_, AppState>,
    format: String,
    from: Option<String>,
    to: Option<String>,
    desc: Option<bool>,
) -> Result<ExportOutcome, String> {
    let storage = state.storage.clone();
    let (rows, total) = tauri::async_runtime::spawn_blocking(move || {
        let rows = storage.list_history(from.clone(), to.clone(), desc.unwrap_or(true), 5000)?;
        let total = storage.history_count_filtered(from, to)?;
        Ok::<_, String>((rows, total))
    })
    .await
    .map_err(|e| e.to_string())??;
    if rows.is_empty() {
        return Err("没有可导出的历史记录".into());
    }
    let (content, default_name, filters) = match format.as_str() {
        "csv" => (
            to_csv(&rows),
            "netvision_history.csv".to_string(),
            vec![("CSV 文件", &["csv"][..])],
        ),
        "json" => {
            let json = serde_json::to_string_pretty(&rows).map_err(|e| e.to_string())?;
            (
                json,
                "netvision_history.json".to_string(),
                vec![("JSON 文件", &["json"][..])],
            )
        }
        _ => return Err("不支持的导出格式".into()),
    };
    let path = save_via_dialog(app, &default_name, filters).await?;
    std::fs::write(&path, content).map_err(|e| format!("写入失败: {e}"))?;
    Ok(ExportOutcome {
        path,
        exported: rows.len().min(u32::MAX as usize) as u32,
        total,
    })
}

/// 为单条测速生成 HTML 报告并保存
#[tauri::command]
pub async fn history_export_report(
    app: AppHandle,
    state: tauri::State<'_, AppState>,
    id: i64,
) -> Result<String, String> {
    let storage = state.storage.clone();
    let detail = tauri::async_runtime::spawn_blocking(move || storage.get_result_detail(id))
        .await
        .map_err(|e| e.to_string())??
        .ok_or_else(|| "记录不存在".to_string())?;
    let html = crate::report::render_html_report(&detail);
    let default_name = format!("NetVision_Report_{}.html", detail.id);
    let path = save_via_dialog(app, &default_name, vec![("HTML 文件", &["html"][..])]).await?;
    std::fs::write(&path, html).map_err(|e| format!("写入失败: {e}"))?;
    Ok(path)
}

async fn save_via_dialog(
    app: AppHandle,
    default_name: &str,
    filters: Vec<(&'static str, &'static [&'static str])>,
) -> Result<String, String> {
    use tauri_plugin_dialog::DialogExt;
    let (tx, rx) = std::sync::mpsc::channel::<Option<tauri_plugin_dialog::FilePath>>();
    let mut builder = app
        .dialog()
        .file()
        .set_file_name(default_name)
        .add_filter(filters[0].0, filters[0].1);
    for (name, exts) in filters.iter().skip(1) {
        builder = builder.add_filter(*name, exts);
    }
    builder.save_file(move |file| {
        let _ = tx.send(file);
    });
    let file = tauri::async_runtime::spawn_blocking(move || {
        rx.recv_timeout(std::time::Duration::from_secs(180))
    })
    .await
    .map_err(|e| e.to_string())?
    .map_err(|_| "保存对话框超时".to_string())?
    .ok_or_else(|| "已取消保存".to_string())?;
    let path: PathBuf = file
        .into_path()
        .map_err(|e| format!("无效的保存路径: {e}"))?;
    let path = if path.extension().is_none() {
        PathBuf::from(format!(
            "{}.{}",
            path.display(),
            default_name.rsplit('.').next().unwrap_or("txt")
        ))
    } else {
        path
    };
    Ok(path.display().to_string())
}

fn to_csv(rows: &[HistoryRow]) -> String {
    let mut out = String::from(
        "测试时间,下载 Mbps,上传 Mbps,空闲延迟 ms,抖动 ms,下载负载延迟 ms,上传负载延迟 ms,丢包率 %,节点,提供商,网络类型,接口,公网 IP,ISP,状态\n",
    );
    for r in rows {
        // 所有字符串字段统一过 csv_field:转义 + 中和公式注入。
        // 节点名来自远端节点列表,ISP/IP 来自网络侧,都不能当作可信文本直接拼接。
        out.push_str(&format!(
            "{},{},{},{},{},{},{},{},{},{},{},{},{},{},{}\n",
            csv_field(&r.created_at),
            fmt2(r.download_mbps),
            r.upload_mbps.map_or_else(String::new, fmt2),
            fmt2(r.idle_ping_ms),
            fmt2(r.jitter_ms),
            r.loaded_down_ms.map_or_else(String::new, fmt2),
            r.loaded_up_ms.map_or_else(String::new, fmt2),
            r.packet_loss_percent.map_or_else(String::new, fmt2),
            csv_field(&r.server_name),
            csv_field(&r.server_provider),
            csv_field(&r.network_type),
            csv_field(&r.interface_name),
            csv_field(r.public_ip.as_deref().unwrap_or_default()),
            csv_field(r.isp.as_deref().unwrap_or_default()),
            csv_field(&r.status),
        ));
    }
    out
}

fn fmt2(v: f64) -> String {
    format!("{v:.2}")
}

/// 表格软件会把以 = + - @ 开头的单元格当公式执行,导出前加前导单引号中和。
/// 前导空白后跟公式前缀也中和;数值字段走 `fmt2`,不会误伤合法负数。
fn neutralize_formula(s: &str) -> Cow<'_, str> {
    let starts_with_formula = matches!(s.trim_start().chars().next(), Some('=' | '+' | '-' | '@'));
    let starts_with_control = matches!(s.chars().next(), Some('\t' | '\r' | '\n'));
    if starts_with_formula || starts_with_control {
        Cow::Owned(format!("'{s}"))
    } else {
        Cow::Borrowed(s)
    }
}

/// CSV 字段:先中和公式注入,再按 RFC 4180 决定是否加引号。
fn csv_field(s: &str) -> String {
    let neutralized = neutralize_formula(s);
    csv_escape(&neutralized)
}

fn csv_escape(s: &str) -> String {
    if s.contains(',') || s.contains('"') || s.contains('\n') || s.contains('\r') {
        format!("\"{}\"", s.replace('"', "\"\""))
    } else {
        s.to_string()
    }
}

// ============================================================
// 网络监控
// ============================================================

#[tauri::command]
pub async fn monitor_set_enabled(
    state: tauri::State<'_, AppState>,
    enabled: bool,
) -> Result<(), String> {
    state.monitor.set_enabled(enabled);
    Ok(())
}

#[tauri::command]
pub async fn monitor_set_paused(
    state: tauri::State<'_, AppState>,
    paused: bool,
) -> Result<(), String> {
    state.monitor.set_paused(paused);
    Ok(())
}

#[tauri::command]
pub fn monitor_history(
    state: tauri::State<AppState>,
    window_secs: u32,
) -> Vec<crate::monitor::MonitorTick> {
    state.monitor.history(window_secs)
}

#[tauri::command]
pub fn monitor_latest(state: tauri::State<AppState>) -> Option<crate::monitor::MonitorTick> {
    state.monitor.latest()
}

/// 监控是否被暂停:前端页面重挂载后需要恢复真实暂停态,不能默认成"未暂停"
#[tauri::command]
pub fn monitor_paused(state: tauri::State<AppState>) -> bool {
    state.monitor.is_paused()
}

// ============================================================
// 网络诊断
// ============================================================

/// 诊断结果落库(仅当设置 save_diag_history 开启)。
/// 失败不向调用方传播:诊断本身已经成功,记录历史不该打断用户操作。
async fn record_diag(
    state: &tauri::State<'_, AppState>,
    kind: &str,
    target: Option<&str>,
    ok: bool,
    summary: String,
    detail: Option<String>,
) {
    if !state.settings.settings.read().await.save_diag_history {
        return;
    }
    let storage = state.storage.clone();
    let created_at = chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string();
    let kind = kind.to_string();
    let target = target.map(|s| s.to_string());
    let res = tokio::task::spawn_blocking(move || {
        storage.insert_diag_run(&crate::storage::DiagRunInput {
            created_at: &created_at,
            kind: &kind,
            target: target.as_deref(),
            ok,
            summary: &summary,
            detail: detail.as_deref(),
        })
    })
    .await;
    if let Ok(Err(e)) = res {
        log::warn!("诊断历史写入失败: {e}");
    }
}

#[tauri::command]
pub async fn diag_history_list(
    state: tauri::State<'_, AppState>,
    kind: Option<String>,
    limit: Option<u32>,
) -> Result<Vec<crate::storage::DiagRunRow>, String> {
    let storage = state.storage.clone();
    tokio::task::spawn_blocking(move || {
        storage.list_diag_runs(kind.as_deref(), limit.unwrap_or(200))
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn diag_history_detail(
    state: tauri::State<'_, AppState>,
    id: i64,
) -> Result<Option<String>, String> {
    let storage = state.storage.clone();
    tokio::task::spawn_blocking(move || storage.diag_run_detail(id))
        .await
        .map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn diag_history_clear(state: tauri::State<'_, AppState>) -> Result<u64, String> {
    let storage = state.storage.clone();
    tokio::task::spawn_blocking(move || storage.clear_diag_runs())
        .await
        .map_err(|e| e.to_string())?
}

#[tauri::command]
pub fn diag_local_network() -> Result<diagnostics::LocalNetInfo, CommandError> {
    let ifaces = diagnostics::collect_interfaces();
    let hostname = hostname::get()
        .map(|h| h.to_string_lossy().to_string())
        .unwrap_or_default();
    Ok(diagnostics::LocalNetInfo {
        interfaces: ifaces,
        hostname,
        os: format!("{} {}", std::env::consts::OS, std::env::consts::ARCH),
    })
}

#[tauri::command]
pub async fn diag_wifi_details() -> Result<Option<diagnostics::WifiDetails>, CommandError> {
    Ok(diagnostics::wifi_details().await)
}

#[tauri::command]
pub async fn diag_public_info(
    state: tauri::State<'_, AppState>,
) -> Result<diagnostics::PublicInfo, CommandError> {
    let allowed = state.settings.settings.read().await.show_public_ip;
    Ok(diagnostics::public_info(allowed).await)
}

#[tauri::command]
pub async fn diag_dns_test() -> Result<diagnostics::DnsTestReport, CommandError> {
    Ok(diagnostics::dns_test().await)
}

#[tauri::command]
pub async fn diag_gateway_test() -> Result<diagnostics::GatewayReport, CommandError> {
    Ok(diagnostics::gateway_test().await)
}

#[tauri::command]
pub async fn diag_tcp_test(
    targets: Option<Vec<String>>,
) -> Result<Vec<diagnostics::TcpResult>, CommandError> {
    // 默认目标集:常见关键服务(均为常量,无注入面)
    let list: Vec<(String, u16)> = targets
        .map(|v| {
            v.iter()
                .filter_map(|t| {
                    let mut it = t.rsplitn(2, ':');
                    let port = it.next()?.parse::<u16>().ok()?;
                    let host = it.next()?.to_string();
                    Some((host, port))
                })
                .collect()
        })
        .unwrap_or_else(|| {
            vec![
                ("speed.cloudflare.com".into(), 443),
                ("www.cloudflare.com".into(), 443),
                ("www.baidu.com".into(), 443),
                ("223.5.5.5".into(), 53),
            ]
        });
    let mut out = Vec::new();
    for (host, port) in list {
        out.push(diagnostics::tcp_connect_test(&host, port).await);
    }
    Ok(out)
}

#[tauri::command]
pub async fn diag_https_test() -> Result<Vec<diagnostics::HttpsResult>, CommandError> {
    let urls = [
        "https://speed.cloudflare.com/__down?bytes=0",
        "https://www.msftconnecttest.com/connecttest.txt",
        "https://www.baidu.com",
    ];
    let mut out = Vec::new();
    for u in urls {
        out.push(diagnostics::https_test(u).await);
    }
    Ok(out)
}

#[tauri::command]
pub async fn diag_traceroute(
    app: AppHandle,
    state: tauri::State<'_, AppState>,
    host: String,
    max_hops: Option<u32>,
) -> Result<Vec<crate::engine::quality::TraceHop>, CommandError> {
    let hops = diagnostics::traceroute(&host, max_hops.unwrap_or(15).clamp(3, 30)).await?;
    // 逐跳事件(前端可实时渲染)
    for h in &hops {
        let _ = app.emit("diag://trace", h);
    }
    // 路由追踪可能被目标/中间设备屏蔽:如实记录条数,不判断成败
    let summary = format!(
        "{} 跳 · 末跳 {}",
        hops.len(),
        hops.iter()
            .rev()
            .find_map(|h| h.ip.clone())
            .unwrap_or_else(|| "*".into())
    );
    record_diag(
        &state,
        "traceroute",
        Some(&host),
        !hops.is_empty(),
        summary,
        serde_json::to_string(&hops).ok(),
    )
    .await;
    Ok(hops)
}

#[tauri::command]
pub async fn diag_ping_test(
    state: tauri::State<'_, AppState>,
    host: String,
    count: Option<u32>,
) -> Result<PingReport, CommandError> {
    let count = count.unwrap_or(8).clamp(2, 30);
    let out = diagnostics::icmp_ping(&host, count, 1500).await;
    let report = PingReport {
        host: host.clone(),
        sent: count,
        loss_percent: out.loss_percent,
        rtts_ms: out.rtts_ms.clone(),
        avg_ms: out.avg_ms,
    };
    let summary = format!(
        "{} 包 / 丢包 {} / 平均 {} ms",
        count,
        out.loss_percent
            .map(|l| format!("{l:.0}%"))
            .unwrap_or_else(|| "未知".into()),
        out.avg_ms
            .map(|v| format!("{v:.1}"))
            .unwrap_or_else(|| "—".into())
    );
    let ok = out.loss_percent.is_some_and(|l| l < 100.0);
    record_diag(
        &state,
        "ping",
        Some(&host),
        ok,
        summary,
        serde_json::to_string(&report).ok(),
    )
    .await;
    Ok(report)
}

#[derive(Serialize, Clone)]
pub struct PingReport {
    pub host: String,
    pub sent: u32,
    pub loss_percent: Option<f64>,
    pub rtts_ms: Vec<f64>,
    pub avg_ms: Option<f64>,
}

#[tauri::command]
pub async fn diag_ipv6_check(
    state: tauri::State<'_, AppState>,
) -> Result<diagnostics::IpVersionCheck, CommandError> {
    let allowed = state.settings.settings.read().await.show_public_ip;
    Ok(diagnostics::ipv6_check(allowed).await)
}

#[tauri::command]
pub async fn diag_route_quality(
    state: tauri::State<'_, AppState>,
    host: String,
    max_hops: Option<u32>,
    probes: Option<u32>,
) -> Result<diagnostics::RouteQualityReport, CommandError> {
    let report = diagnostics::route_quality(&host, max_hops.unwrap_or(12), probes.unwrap_or(6))
        .await
        .map_err(CommandError::from)?;
    let summary = if report.hops.is_empty() {
        "未获得任何跳点".to_string()
    } else {
        // 取末端有效跳点的延迟/丢包作为摘要,便于列表快速判断链路质量
        let last = report
            .hops
            .iter()
            .rev()
            .find(|h| h.avg_ms.is_some() || h.loss_percent.is_some());
        match last {
            Some(h) => format!(
                "{} 跳 · 第 {} 跳 {} / 丢包 {} / 平均 {} ms",
                report.hops.len(),
                h.hop,
                h.ip.clone().unwrap_or_else(|| "*".into()),
                h.loss_percent
                    .map(|v| format!("{v:.0}%"))
                    .unwrap_or_else(|| "未知".into()),
                h.avg_ms
                    .map(|v| format!("{v:.1}"))
                    .unwrap_or_else(|| "—".into())
            ),
            None => format!("{} 跳 · 全部超时", report.hops.len()),
        }
    };
    let ok = report
        .hops
        .iter()
        .any(|h| h.loss_percent.is_some_and(|l| l < 100.0));
    record_diag(
        &state,
        "route_quality",
        Some(&host),
        ok,
        summary,
        serde_json::to_string(&report).ok(),
    )
    .await;
    Ok(report)
}

#[tauri::command]
pub async fn diag_dns_consistency(
    state: tauri::State<'_, AppState>,
    domain: String,
) -> Result<diagnostics::DnsConsistencyReport, CommandError> {
    let domain = domain.trim().to_string();
    if domain.is_empty() {
        return Err(CommandError::new("invalid_input", "域名不能为空"));
    }
    if !diagnostics::valid_host(&domain) {
        return Err(CommandError::new("invalid_host", "无效的域名"));
    }
    let report = diagnostics::dns_consistency(&domain).await;
    let ok = report.verdict != "可疑劫持" && report.verdict != "异常";
    let summary = format!(
        "{} · {} 个解析器{}{}",
        report.verdict,
        report.entries.len(),
        if report.note.is_some() { " · " } else { "" },
        report.note.clone().unwrap_or_default()
    );
    record_diag(
        &state,
        "dns_consistency",
        Some(&domain),
        ok,
        summary,
        serde_json::to_string(&report).ok(),
    )
    .await;
    Ok(report)
}

#[tauri::command]
pub async fn diag_nat_type(
    state: tauri::State<'_, AppState>,
) -> Result<diagnostics::NatReport, CommandError> {
    let report = diagnostics::nat_type().await;
    let ok = report.nat_type != "未知";
    let summary = format!(
        "{}{}",
        report.nat_type,
        report
            .note
            .as_ref()
            .map(|n| format!(" · {n}"))
            .unwrap_or_default()
    );
    record_diag(
        &state,
        "nat",
        None,
        ok,
        summary,
        serde_json::to_string(&report).ok(),
    )
    .await;
    Ok(report)
}

#[tauri::command]
pub async fn diag_connectivity_grid() -> Result<Vec<diagnostics::ConnectivityEntry>, CommandError> {
    Ok(diagnostics::connectivity_grid().await)
}

#[tauri::command]
pub fn pingmon_start(
    app: AppHandle,
    state: tauri::State<AppState>,
    host: String,
) -> Result<(), String> {
    state.pingmon.start(app, host)
}

#[tauri::command]
pub fn pingmon_stop(state: tauri::State<AppState>) {
    state.pingmon.stop();
}

#[tauri::command]
pub fn pingmon_host(state: tauri::State<AppState>) -> Option<String> {
    state.pingmon.current_host()
}

#[tauri::command]
pub async fn batchtest_start(
    app: AppHandle,
    state: tauri::State<'_, AppState>,
    targets: Vec<String>,
    count: Option<u32>,
) -> Result<usize, String> {
    // 异步包装同步启动:避免 FnOnce 泛型问题
    state.batchtest.start(app, targets, count.unwrap_or(10))
}

#[tauri::command]
pub fn batchtest_stop(state: tauri::State<AppState>) {
    state.batchtest.stop();
}

#[tauri::command]
pub fn batchtest_running(state: tauri::State<AppState>) -> bool {
    state.batchtest.is_running()
}

#[tauri::command]
pub async fn dns_distribution(
    domain: String,
) -> Result<crate::batchtest::DnsDistributionReport, String> {
    crate::batchtest::dns_distribution(&domain).await
}

/// 快速分享文本(剪贴板)
#[tauri::command]
pub fn copy_result_text(result: crate::engine::model::TestResult) -> Result<String, String> {
    let mut s = String::new();
    s.push_str(&format!("NetVision Pro 测速结果({})\n", result.started_at));
    s.push_str(&format!(
        "↓ 下载:{:.2} Mbps(峰值 {:.1})\n",
        result.download_mbps, result.download_peak_mbps
    ));
    if let Some(up) = result.upload_mbps {
        s.push_str(&format!(
            "↑ 上传:{:.2} Mbps(峰值 {:.1})\n",
            up,
            result.upload_peak_mbps.unwrap_or(0.0)
        ));
    }
    s.push_str(&format!(
        "延迟:{:.1} ms / 抖动:{:.1} ms / 丢包:{}\n",
        result.idle_ping_ms,
        result.jitter_ms,
        result
            .packet_loss_percent
            .map(|l| format!("{:.1}%", l))
            .unwrap_or_else(|| "未知".into())
    ));
    if let Some(q) = &result.quality {
        s.push_str(&format!("质量:{}({}/100)\n", q.rating, q.score));
    }
    if let Some(srv) = &result.server {
        s.push_str(&format!("节点:{}", srv.name));
    }
    Ok(s)
}

// ============================================================
// 自动更新
// ============================================================

/// 可用更新摘要(无可更新时为 None;数据来自更新源 latest.json)
#[derive(Debug, Clone, Serialize)]
pub struct UpdaterInfo {
    pub version: String,
    pub notes: Option<String>,
    pub pub_date: Option<String>,
}

/// 检查更新。端点不可达、签名校验失败等都如实报错,不静默当作"无更新"。
#[tauri::command]
pub async fn updater_check(app: AppHandle) -> Result<Option<UpdaterInfo>, String> {
    use tauri_plugin_updater::UpdaterExt as _;
    let updater = app
        .updater_builder()
        .build()
        .map_err(|e| format!("更新器初始化失败: {e}"))?;
    let update = updater
        .check()
        .await
        .map_err(|e| format!("检查更新失败: {e}"))?;
    Ok(update.map(|u| UpdaterInfo {
        version: u.version.clone(),
        notes: u.body.clone(),
        pub_date: u.date.map(|d| d.to_string()),
    }))
}

/// 下载并安装更新:进度经 updater://progress 事件回传,完成后重启应用。
/// (Windows NSIS 以 passive 模式安装,进程由安装器接管)
#[tauri::command]
pub async fn updater_install(app: AppHandle) -> Result<(), String> {
    use tauri_plugin_updater::UpdaterExt as _;
    let updater = app
        .updater_builder()
        .build()
        .map_err(|e| format!("更新器初始化失败: {e}"))?;
    let update = updater
        .check()
        .await
        .map_err(|e| format!("检查更新失败: {e}"))?;
    let Some(update) = update else {
        return Err("当前已是最新版本,无需安装".into());
    };

    let progress_app = app.clone();
    let mut downloaded: u64 = 0;
    let on_chunk = move |chunk: usize, total: Option<u64>| {
        downloaded += chunk as u64;
        let _ = progress_app.emit(
            "updater://progress",
            serde_json::json!({ "downloaded": downloaded, "total": total }),
        );
    };
    let finish_app = app.clone();
    let on_finish = move || {
        let _ = finish_app.emit("updater://progress", serde_json::json!({ "done": true }));
    };
    update
        .download_and_install(on_chunk, on_finish)
        .await
        .map_err(|e| format!("更新下载/安装失败: {e}"))?;
    app.restart();
}

use tauri::Emitter as _;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_csv_escape() {
        assert_eq!(csv_escape("plain"), "plain");
        assert_eq!(csv_escape("a,b"), "\"a,b\"");
        assert_eq!(csv_escape("say \"hi\""), "\"say \"\"hi\"\"\"");
    }

    #[test]
    fn test_csv_field_neutralizes_formula_injection() {
        // 远端节点名/ISP 属于不可信文本,表格软件不得把 = + - @ 当公式执行
        assert_eq!(csv_field("=cmd|calc"), "'=cmd|calc");
        assert_eq!(csv_field("+1+1"), "'+1+1");
        assert_eq!(csv_field("-1+1"), "'-1+1");
        assert_eq!(csv_field("@SUM(A1)"), "'@SUM(A1)");
        assert_eq!(csv_field("  =SUM(A1)"), "'  =SUM(A1)");
        assert_eq!(csv_field("\t=SUM(A1)"), "'\t=SUM(A1)");
        // 正常文本不受影响
        assert_eq!(csv_field("China Mobile"), "China Mobile");
        // 公式前缀 + 逗号:先中和再加引号
        assert_eq!(csv_field("=a,b"), "\"'=a,b\"");
        // 空值安全
        assert_eq!(csv_field(""), "");
    }

    #[test]
    fn test_to_csv_escapes_comma_bearing_isp() {
        let row = HistoryRow {
            id: 1,
            created_at: "2026-11-10 12:00:00".into(),
            download_mbps: 100.0,
            upload_mbps: Some(50.0),
            idle_ping_ms: 10.0,
            jitter_ms: 1.0,
            loaded_down_ms: Some(12.0),
            loaded_up_ms: Some(11.0),
            packet_loss_percent: Some(0.0),
            server_name: "Tokyo, Japan (Example)".into(),
            server_provider: "librespeed".into(),
            server_url: "https://example.com".into(),
            network_type: "ethernet".into(),
            interface_name: "Ethernet".into(),
            public_ip: Some("203.0.113.9".into()),
            isp: Some("China Mobile Communications Group Co., Ltd.".into()),
            status: "completed".into(),
        };
        let csv = to_csv(&[row]);
        let data_line = csv.lines().nth(1).unwrap();
        // ISP 含逗号必须被引号包裹,否则整行列数错乱
        assert!(data_line.contains("\"China Mobile Communications Group Co., Ltd.\""));
        assert!(data_line.contains("\"Tokyo, Japan (Example)\""));
    }

    #[test]
    fn test_speedtest_service_whitelist() {
        // 已知服务均返回 HTTPS 固定入口
        for id in [
            "speedtest_cn",
            "ookla",
            "fast",
            "cloudflare",
            "telecom_gd",
            "unicom",
            "mobile_bj",
        ] {
            let url = speedtest_service_url(id).unwrap_or_else(|| panic!("缺少服务 {id}"));
            assert!(url.starts_with("https://"), "服务 {id} 未强制 HTTPS");
        }
        // 未知 / 任意输入必须拒绝(防止被当作任意 URL 打开器)
        assert!(speedtest_service_url("").is_none());
        assert!(speedtest_service_url("https://evil.example/").is_none());
        assert!(speedtest_service_url("../../etc/passwd").is_none());
        assert!(speedtest_service_url("file:///C:/Windows").is_none());
        assert!(speedtest_service_url("ookla; shutdown").is_none());
    }
}

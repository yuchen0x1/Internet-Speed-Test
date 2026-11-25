//! Tauri 命令层:前端唯一入口。所有业务在引擎/诊断/存储层完成,本层仅做编排与授权检查。

use std::borrow::Cow;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use serde::{Deserialize, Serialize};
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

/// 可迁移的设置备份格式。使用显式格式名和版本号，避免把任意 JSON
/// 当作设置文件直接覆盖当前配置。
#[derive(Debug, Clone, Deserialize, Serialize)]
struct SettingsBackup {
    format: String,
    schema_version: u32,
    exported_at: String,
    app_version: String,
    settings: AppSettings,
}

const SETTINGS_BACKUP_FORMAT: &str = "netvision-pro-settings";
const SETTINGS_BACKUP_VERSION: u32 = 1;
const MAX_IMPORTED_CUSTOM_NODES: usize = 64;

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
    if !["auto", "direct", "manual"].contains(&s.proxy_mode.as_str()) {
        s.proxy_mode = "auto".into();
    }
    if s.proxy_mode != "manual" || !s.proxy_url.trim().is_empty() {
        crate::network::ProxyPolicy::from_parts(&s.proxy_mode, &s.proxy_url, &s.proxy_bypass)?;
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

fn normalize_imported_settings(mut s: AppSettings) -> Result<AppSettings, String> {
    // Keep the same bounds as the regular settings command so a hand-edited
    // backup cannot create an unbounded worker or refresh loop.
    s.down_concurrency = s.down_concurrency.clamp(1, 16);
    s.up_concurrency = s.up_concurrency.clamp(1, 8);
    s.test_duration_secs = s.test_duration_secs.clamp(5, 30);
    s.anim_intensity = s.anim_intensity.clamp(0.0, 1.5);
    s.refresh_interval_ms = s.refresh_interval_ms.clamp(200, 5000);
    s.auto_test_interval_minutes = s.auto_test_interval_minutes.clamp(15, 1440);
    s.update_check_interval_hours = s.update_check_interval_hours.clamp(6, 168);
    s.alert_threshold_ms = s.alert_threshold_ms.clamp(50, 1000);
    s.prometheus_port = s.prometheus_port.clamp(1024, 65535);
    s.retention_days = match s.retention_days {
        0 | 30 | 90 | 180 | 365 => s.retention_days,
        value if value < 30 => 30,
        value if value < 90 => 90,
        value if value < 180 => 180,
        value if value < 365 => 180,
        _ => 365,
    };
    if !["dark", "light", "system"].contains(&s.theme.as_str()) {
        s.theme = "dark".into();
    }
    if !["auto", "manual"].contains(&s.node_mode.as_str()) {
        s.node_mode = "auto".into();
    }
    if !["auto", "direct", "manual"].contains(&s.proxy_mode.as_str()) {
        s.proxy_mode = "auto".into();
    }
    if s.proxy_mode != "manual" || !s.proxy_url.trim().is_empty() {
        crate::network::ProxyPolicy::from_parts(&s.proxy_mode, &s.proxy_url, &s.proxy_bypass)?;
    }

    if s.custom_nodes.len() > MAX_IMPORTED_CUSTOM_NODES {
        return Err(format!(
            "设置备份包含过多自定义节点(最多 {MAX_IMPORTED_CUSTOM_NODES} 个)"
        ));
    }
    let mut nodes = Vec::with_capacity(s.custom_nodes.len());
    for mut node in s.custom_nodes {
        let base = util::validate_server_url(&node.base_url, node.allow_http)?;
        node.base_url = base.clone();
        node.name = if node.name.trim().is_empty() {
            util::extract_host(&base).unwrap_or_else(|| "自定义节点".into())
        } else {
            node.name.trim().chars().take(120).collect()
        };
        node.provider = "custom".into();
        node.id = util::node_id(&base, &node.name);
        node.alive = None;
        node.latency_ms = None;
        if nodes.iter().any(|n: &ServerNodeInfo| n.base_url == base) {
            return Err(format!("设置备份包含重复自定义节点: {base}"));
        }
        nodes.push(node);
    }
    s.custom_nodes = nodes;
    if s.node_mode == "manual" {
        s.default_node_url = crate::settings::validate_manual_node(&s, &s.default_node_url)?;
    }
    Ok(s)
}

async fn apply_settings_snapshot(
    app: &AppHandle,
    state: &AppState,
    s: AppSettings,
) -> Result<AppSettings, String> {
    let mut guard = state.settings.settings.write().await;
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
    let storage = state.storage.clone();
    let connected_getter: Arc<dyn Fn() -> bool + Send + Sync> =
        Arc::new(|| crate::diagnostics::current_net_status().connected);
    state.prometheus.apply(
        s.prometheus_enabled,
        s.prometheus_port,
        storage,
        connected_getter,
    );
    Ok(s)
}

/// 将设置导出为带版本号的 JSON 文件，便于迁移到另一台电脑。
#[tauri::command]
pub async fn settings_export(
    app: AppHandle,
    state: tauri::State<'_, AppState>,
) -> Result<String, String> {
    let settings = state.settings.settings.read().await.clone();
    let backup = SettingsBackup {
        format: SETTINGS_BACKUP_FORMAT.into(),
        schema_version: SETTINGS_BACKUP_VERSION,
        exported_at: chrono::Local::now().to_rfc3339(),
        app_version: env!("CARGO_PKG_VERSION").into(),
        settings,
    };
    let content = serde_json::to_string_pretty(&backup).map_err(|e| e.to_string())?;
    let path = save_via_dialog(
        app,
        "netvision-settings.json",
        vec![("JSON 文件", &["json"][..])],
    )
    .await?;
    std::fs::write(&path, content).map_err(|e| format!("写入设置备份失败: {e}"))?;
    Ok(path)
}

/// 从用户选择的 JSON 备份恢复设置。解析、校验和原子写入全部在替换当前设置前完成。
#[tauri::command]
pub async fn settings_import(
    app: AppHandle,
    state: tauri::State<'_, AppState>,
) -> Result<AppSettings, String> {
    let path = open_via_dialog(app.clone(), vec![("JSON 文件", &["json"][..])]).await?;
    let text = tokio::fs::read_to_string(&path)
        .await
        .map_err(|e| format!("读取设置备份失败: {e}"))?;
    let backup: SettingsBackup =
        serde_json::from_str(&text).map_err(|e| format!("设置备份格式无效: {e}"))?;
    if backup.format != SETTINGS_BACKUP_FORMAT {
        return Err("不是 NetVision Pro 设置备份文件".into());
    }
    if backup.schema_version != SETTINGS_BACKUP_VERSION {
        return Err(format!("不支持的设置备份版本: {}", backup.schema_version));
    }
    let settings = normalize_imported_settings(backup.settings)?;
    apply_settings_snapshot(&app, &state, settings).await
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
    let custom = settings.custom_nodes.clone();
    let registry = crate::engine::selector::ServerRegistry::from_settings(&settings)?;
    drop(settings);
    Ok(registry.list_servers(&custom, &favorites).await)
}

/// 刷新节点:重新拉列表 + 并发探测全部节点(返回含延迟与可用性)
#[tauri::command]
pub async fn servers_refresh(
    state: tauri::State<'_, AppState>,
) -> Result<Vec<ServerNodeInfo>, String> {
    let settings = state.settings.settings.read().await;
    let custom = settings.custom_nodes.clone();
    let registry = crate::engine::selector::ServerRegistry::from_settings(&settings)?;
    drop(settings);
    let favorites = state.storage.favorites_map();
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
#[allow(clippy::too_many_arguments)]
pub async fn history_list(
    state: tauri::State<'_, AppState>,
    from: Option<String>,
    to: Option<String>,
    query: Option<String>,
    status: Option<String>,
    desc: Option<bool>,
    limit: Option<u32>,
    cursor_id: Option<i64>,
) -> Result<Vec<HistoryRow>, String> {
    let storage = state.storage.clone();
    tauri::async_runtime::spawn_blocking(move || {
        storage.list_history_page_filtered(
            from,
            to,
            query,
            status,
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
    query: Option<String>,
    status: Option<String>,
) -> Result<u32, String> {
    let storage = state.storage.clone();
    tauri::async_runtime::spawn_blocking(move || {
        storage.history_count_filtered_with_query(from, to, query, status)
    })
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
    query: Option<String>,
    status: Option<String>,
) -> Result<crate::storage::HistorySummary, String> {
    let storage = state.storage.clone();
    tauri::async_runtime::spawn_blocking(move || {
        storage.history_summary_filtered(from, to, query, status)
    })
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
#[allow(clippy::too_many_arguments)]
pub async fn history_export(
    app: AppHandle,
    state: tauri::State<'_, AppState>,
    format: String,
    from: Option<String>,
    to: Option<String>,
    query: Option<String>,
    status: Option<String>,
    desc: Option<bool>,
) -> Result<ExportOutcome, String> {
    let storage = state.storage.clone();
    let (rows, total) = tauri::async_runtime::spawn_blocking(move || {
        let rows = storage.list_history_filtered(
            from.clone(),
            to.clone(),
            query.clone(),
            status.clone(),
            desc.unwrap_or(true),
            5000,
        )?;
        let total = storage.history_count_filtered_with_query(from, to, query, status)?;
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

async fn open_via_dialog(
    app: AppHandle,
    filters: Vec<(&'static str, &'static [&'static str])>,
) -> Result<String, String> {
    use tauri_plugin_dialog::DialogExt;
    let (tx, rx) = std::sync::mpsc::channel::<Option<tauri_plugin_dialog::FilePath>>();
    let mut builder = app.dialog().file().add_filter(filters[0].0, filters[0].1);
    for (name, exts) in filters.iter().skip(1) {
        builder = builder.add_filter(*name, exts);
    }
    builder.pick_file(move |file| {
        let _ = tx.send(file);
    });
    let file = tauri::async_runtime::spawn_blocking(move || {
        rx.recv_timeout(std::time::Duration::from_secs(180))
    })
    .await
    .map_err(|e| e.to_string())?
    .map_err(|_| "打开对话框超时".to_string())?
    .ok_or_else(|| "已取消导入".to_string())?;
    let path: PathBuf = file
        .into_path()
        .map_err(|e| format!("无效的设置备份路径: {e}"))?;
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

/// Return epoch-aligned monitor summaries for the selected window. The
/// allow-list keeps the UI contract predictable and prevents accidental
/// per-sample aggregation requests from growing without bound.
#[tauri::command]
pub fn monitor_aggregate(
    state: tauri::State<AppState>,
    window_secs: u32,
    bucket_secs: u32,
) -> Result<Vec<crate::monitor::MonitorBucket>, String> {
    if !matches!(bucket_secs, 60 | 300 | 3600) {
        return Err("监控聚合粒度只支持 60、300 或 3600 秒".into());
    }
    Ok(state.monitor.aggregate(window_secs, bucket_secs))
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

/// 将当前监控时间窗保存为 CSV 或 JSON。导出使用原始缓冲样本，窗口和格式均在后端限制。
#[tauri::command]
pub async fn monitor_export(
    app: AppHandle,
    state: tauri::State<'_, AppState>,
    window_secs: u32,
    format: String,
) -> Result<String, String> {
    let format = format.trim().to_ascii_lowercase();
    if !matches!(format.as_str(), "csv" | "json") {
        return Err("不支持的监控导出格式".into());
    }
    let ticks = state.monitor.export_history(window_secs);
    if ticks.is_empty() {
        return Err("当前时间窗没有可导出的监控数据".into());
    }
    let (content, default_name, filters) = if format == "csv" {
        (
            monitor_csv(&ticks),
            format!("netvision_monitor_{}s.csv", window_secs.clamp(10, 3600)),
            vec![("CSV 文件", &["csv"][..])],
        )
    } else {
        (
            serde_json::to_string_pretty(&ticks).map_err(|e| e.to_string())?,
            format!("netvision_monitor_{}s.json", window_secs.clamp(10, 3600)),
            vec![("JSON 文件", &["json"][..])],
        )
    };
    let path = save_via_dialog(app, &default_name, filters).await?;
    std::fs::write(&path, content).map_err(|e| format!("写入监控导出失败: {e}"))?;
    Ok(path)
}

fn monitor_csv(ticks: &[crate::monitor::MonitorTick]) -> String {
    let mut out = String::from(
        "timestamp_ms,rx_bps,tx_bps,rx_total,tx_total,interface_count,interfaces_json\n",
    );
    for tick in ticks {
        let interfaces = serde_json::to_string(&tick.ifaces).unwrap_or_else(|_| "[]".into());
        out.push_str(&format!(
            "{},{},{},{},{},{},{}\n",
            tick.ts_ms,
            fmt2(tick.rx_bps),
            fmt2(tick.tx_bps),
            tick.rx_total,
            tick.tx_total,
            tick.ifaces.len(),
            csv_escape(&interfaces),
        ));
    }
    out
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

fn proxy_policy_from_settings(
    settings: &AppSettings,
) -> Result<crate::network::ProxyPolicy, CommandError> {
    crate::network::ProxyPolicy::from_parts(
        &settings.proxy_mode,
        &settings.proxy_url,
        &settings.proxy_bypass,
    )
    .map_err(CommandError::from)
}

async fn diagnostic_proxy_policy(
    state: &AppState,
) -> Result<(bool, crate::network::ProxyPolicy), CommandError> {
    let settings = state.settings.settings.read().await.clone();
    let policy = proxy_policy_from_settings(&settings)?;
    Ok((settings.show_public_ip, policy))
}

#[tauri::command]
pub async fn diag_public_info(
    state: tauri::State<'_, AppState>,
) -> Result<diagnostics::PublicInfo, CommandError> {
    let settings = state.settings.settings.read().await.clone();
    if !settings.show_public_ip {
        return Ok(diagnostics::PublicInfo {
            allowed: false,
            ipv4: None,
            ipv6: None,
            isp: None,
            edge_colo: None,
            country: None,
            note: None,
            error: None,
        });
    }
    match proxy_policy_from_settings(&settings) {
        Ok(policy) => Ok(diagnostics::public_info(true, &policy).await),
        Err(e) => Ok(diagnostics::PublicInfo {
            allowed: true,
            ipv4: None,
            ipv6: None,
            isp: None,
            edge_colo: None,
            country: None,
            note: None,
            error: Some(e.message),
        }),
    }
}

#[tauri::command]
pub async fn diag_dns_test(
    state: tauri::State<'_, AppState>,
) -> Result<diagnostics::DnsTestReport, CommandError> {
    let report = diagnostics::dns_test().await;
    let domains: Vec<_> = report
        .domains
        .iter()
        .filter(|d| d.domain != "__hint__")
        .collect();
    let domain_ok = domains.iter().filter(|d| d.ok).count();
    let server_ok = report.server_probes.iter().filter(|p| p.ok).count();
    let summary = format!(
        "域名解析 {domain_ok}/{} · DNS 服务器响应 {server_ok}/{}",
        domains.len(),
        report.server_probes.len()
    );
    record_diag(
        &state,
        "dns",
        None,
        domain_ok > 0 || server_ok > 0,
        summary,
        serde_json::to_string(&report).ok(),
    )
    .await;
    Ok(report)
}

#[tauri::command]
pub async fn diag_gateway_test(
    state: tauri::State<'_, AppState>,
) -> Result<diagnostics::GatewayReport, CommandError> {
    let report = diagnostics::gateway_test().await;
    let summary = format!(
        "{} · 丢包 {} · 平均 {} ms",
        if report.reachable {
            "可达"
        } else {
            "不可达或未知"
        },
        report
            .loss_percent
            .map(|v| format!("{v:.0}%"))
            .unwrap_or_else(|| "未知".into()),
        report
            .avg_ms
            .map(|v| format!("{v:.1}"))
            .unwrap_or_else(|| "—".into())
    );
    record_diag(
        &state,
        "gateway",
        report.gateway.as_deref(),
        report.reachable,
        summary,
        serde_json::to_string(&report).ok(),
    )
    .await;
    Ok(report)
}

#[tauri::command]
pub async fn diag_tcp_test(
    state: tauri::State<'_, AppState>,
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
    let ok_count = out.iter().filter(|r| r.ok).count();
    let summary = format!("TCP 连接成功 {ok_count}/{}", out.len());
    record_diag(
        &state,
        "tcp",
        None,
        ok_count > 0,
        summary,
        serde_json::to_string(&out).ok(),
    )
    .await;
    Ok(out)
}

#[tauri::command]
pub async fn diag_https_test(
    state: tauri::State<'_, AppState>,
) -> Result<Vec<diagnostics::HttpsResult>, CommandError> {
    let (_, policy) = diagnostic_proxy_policy(state.inner()).await?;
    let urls = [
        "https://speed.cloudflare.com/__down?bytes=0",
        "https://www.msftconnecttest.com/connecttest.txt",
        "https://www.baidu.com",
    ];
    let mut out = Vec::new();
    for u in urls {
        out.push(diagnostics::https_test(u, &policy).await);
    }
    let ok_count = out.iter().filter(|r| r.ok).count();
    let summary = format!("HTTPS 请求成功 {ok_count}/{}", out.len());
    record_diag(
        &state,
        "https",
        None,
        ok_count > 0,
        summary,
        serde_json::to_string(&out).ok(),
    )
    .await;
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
    let (allowed, policy) = diagnostic_proxy_policy(state.inner()).await?;
    let report = diagnostics::ipv6_check(allowed, &policy).await;
    let summary = if report.available {
        "IPv6 连接可用".to_string()
    } else {
        format!(
            "IPv6 不可用{}",
            report
                .error
                .as_ref()
                .map(|e| format!(" · {e}"))
                .unwrap_or_default()
        )
    };
    record_diag(
        &state,
        "ipv6",
        None,
        report.available,
        summary,
        serde_json::to_string(&report).ok(),
    )
    .await;
    Ok(report)
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
pub async fn diag_connectivity_grid(
    state: tauri::State<'_, AppState>,
) -> Result<Vec<diagnostics::ConnectivityEntry>, CommandError> {
    let report = diagnostics::connectivity_grid().await;
    let ok_count = report.iter().filter(|r| r.ok).count();
    let summary = format!("服务连通 {ok_count}/{}", report.len());
    record_diag(
        &state,
        "connectivity",
        None,
        ok_count > 0,
        summary,
        serde_json::to_string(&report).ok(),
    )
    .await;
    Ok(report)
}

/// 导出当前诊断页快照。报告来自前端已展示的结果，Rust 端负责统一脱敏、格式化和文件写入。
/// 结构化网络标识字段会替换为 [已脱敏]，错误/摘要字段保留，便于工单定位失败原因。
#[tauri::command]
pub async fn diag_export_report(
    app: AppHandle,
    format: String,
    report: serde_json::Value,
) -> Result<String, String> {
    if !report.is_object() {
        return Err("诊断报告必须是 JSON 对象".into());
    }
    let mut sanitized = report;
    redact_diag_value(&mut sanitized, None);
    let json =
        serde_json::to_string_pretty(&sanitized).map_err(|e| format!("生成诊断报告失败: {e}"))?;
    if json.len() > 2 * 1024 * 1024 {
        return Err("诊断报告超过 2 MiB，无法导出".into());
    }
    let (content, default_name, filters) = match format.as_str() {
        "json" => (
            json.clone(),
            "netvision-diagnostics.json".to_string(),
            vec![("JSON 文件", &["json"][..])],
        ),
        "html" => (
            render_diag_html(&json),
            "netvision-diagnostics.html".to_string(),
            vec![("HTML 文件", &["html"][..])],
        ),
        _ => return Err("不支持的诊断报告格式".into()),
    };
    let path = save_via_dialog(app, &default_name, filters).await?;
    std::fs::write(&path, content).map_err(|e| format!("写入诊断报告失败: {e}"))?;
    Ok(path)
}

fn redact_diag_value(value: &mut serde_json::Value, key: Option<&str>) {
    if key.is_some_and(is_sensitive_diag_key) {
        match value {
            serde_json::Value::Null => {}
            serde_json::Value::Array(items) => {
                for item in items {
                    if !item.is_null() {
                        *item = serde_json::Value::String("[已脱敏]".into());
                    }
                }
            }
            _ => {
                *value = serde_json::Value::String("[已脱敏]".into());
            }
        }
        return;
    }
    match value {
        serde_json::Value::Object(map) => {
            for (name, child) in map.iter_mut() {
                redact_diag_value(child, Some(name));
            }
        }
        serde_json::Value::Array(items) => {
            for child in items {
                redact_diag_value(child, None);
            }
        }
        _ => {}
    }
}

fn is_sensitive_diag_key(key: &str) -> bool {
    let key = key.to_ascii_lowercase();
    key.ends_with("_host")
        || key.ends_with("_domain")
        || key.ends_with("_url")
        || key.ends_with("_ip")
        || key.ends_with("_address")
        || matches!(
            key.as_str(),
            "hostname"
                | "mac"
                | "bssid"
                | "ssid"
                | "ipv4"
                | "ipv6"
                | "public_ip"
                | "public_address"
                | "mapped_ip"
                | "gateway"
                | "dns_servers"
                | "host"
                | "target"
                | "url"
                | "server"
                | "domain"
                | "ip"
                | "token"
                | "secret"
                | "password"
                | "authorization"
                | "api_key"
        )
}

fn render_diag_html(json: &str) -> String {
    format!(
        "<!doctype html><html lang=\"zh-CN\"><head><meta charset=\"utf-8\"><title>NetVision Pro 诊断报告</title><style>body{{font-family:Segoe UI,Microsoft YaHei,sans-serif;background:#10141d;color:#e6edf7;margin:32px}}pre{{white-space:pre-wrap;word-break:break-word;background:#171e2b;border:1px solid #2a3950;border-radius:10px;padding:18px;line-height:1.55}}h1{{font-size:22px}}</style></head><body><h1>NetVision Pro 诊断报告</h1><p>结构化网络标识已脱敏；错误和摘要字段保留。</p><pre>{}</pre></body></html>",
        escape_html(json)
    )
}

fn escape_html(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
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
pub fn batchtest_snapshot(
    state: tauri::State<AppState>,
) -> Option<crate::batchtest::BatchSnapshot> {
    state.batchtest.snapshot()
}

fn batch_csv(snapshot: &crate::batchtest::BatchSnapshot) -> String {
    let mut csv = String::from("\u{feff}开始时间,状态,目标,计划包数,已发包,收到,丢包率 %,最快 ms,最慢 ms,平均 ms,最新 ms,样本 ms (null=超时)\r\n");
    for target in &snapshot.targets {
        let r = snapshot.results.iter().find(|r| &r.target == target);
        let sent = r.map_or(0, |r| r.sent);
        csv.push_str(
            &[
                csv_field(&snapshot.started_at),
                csv_field(if snapshot.cancelled {
                    "cancelled"
                } else {
                    "completed"
                }),
                csv_field(target),
                snapshot.count.to_string(),
                sent.to_string(),
                r.map_or(0, |r| r.recv).to_string(),
                r.filter(|_| sent > 0)
                    .map_or_else(String::new, |r| fmt2(r.loss_percent)),
                r.and_then(|r| r.min_ms).map_or_else(String::new, fmt2),
                r.and_then(|r| r.max_ms).map_or_else(String::new, fmt2),
                r.and_then(|r| r.avg_ms).map_or_else(String::new, fmt2),
                r.and_then(|r| r.latest_ms).map_or_else(String::new, fmt2),
                csv_field(&r.map_or_else(
                    || "[]".into(),
                    |r| serde_json::to_string(&r.samples).unwrap_or_default(),
                )),
            ]
            .join(","),
        );
        csv.push_str("\r\n");
    }
    csv
}

#[tauri::command]
pub async fn batchtest_export(
    app: AppHandle,
    state: tauri::State<'_, AppState>,
    run_id: u64,
    format: String,
) -> Result<String, String> {
    let snapshot = state.batchtest.snapshot().ok_or("没有可导出的拨测")?;
    if snapshot.run_id != run_id || snapshot.running {
        return Err("拨测仍在运行或结果已更新，请等待结束后重试".into());
    }
    let (content, name, filters) = match format.as_str() {
        "csv" => (
            batch_csv(&snapshot),
            "netvision_batch.csv",
            vec![("CSV 文件", &["csv"][..])],
        ),
        "json" => (
            serde_json::to_string_pretty(&snapshot).map_err(|e| e.to_string())?,
            "netvision_batch.json",
            vec![("JSON 文件", &["json"][..])],
        ),
        _ => return Err("不支持的导出格式".into()),
    };
    let path = save_via_dialog(app, name, filters).await?;
    std::fs::write(&path, content).map_err(|e| format!("写入失败: {e}"))?;
    Ok(path)
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
pub async fn updater_install(app: AppHandle, expected_version: String) -> Result<(), String> {
    let _guard = UpdateInstallGuard::acquire()?;
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
    validate_update_selection(&expected_version, &update.version)?;

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

static UPDATE_INSTALLING: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
struct UpdateInstallGuard;
impl UpdateInstallGuard {
    fn acquire() -> Result<Self, String> {
        UPDATE_INSTALLING
            .compare_exchange(
                false,
                true,
                std::sync::atomic::Ordering::SeqCst,
                std::sync::atomic::Ordering::SeqCst,
            )
            .map(|_| Self)
            .map_err(|_| "更新安装已在进行中".into())
    }
}
impl Drop for UpdateInstallGuard {
    fn drop(&mut self) {
        UPDATE_INSTALLING.store(false, std::sync::atomic::Ordering::SeqCst);
    }
}
fn validate_update_selection(expected: &str, actual: &str) -> Result<(), String> {
    if expected.is_empty() || expected != actual {
        return Err("可用版本已变化，请重新检查更新并确认版本".into());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn updater_requires_reviewed_version_and_serializes_installation() {
        assert!(validate_update_selection("1.15.0", "1.15.1").is_err());
        assert!(validate_update_selection("", "1.15.0").is_err());
        assert!(validate_update_selection("1.15.0", "1.15.0").is_ok());
        let guard = UpdateInstallGuard::acquire().unwrap();
        assert!(UpdateInstallGuard::acquire().is_err());
        drop(guard);
        assert!(UpdateInstallGuard::acquire().is_ok());
    }

    #[test]
    fn diagnostic_proxy_policy_uses_saved_settings() {
        let mut settings = AppSettings::default();
        settings.proxy_mode = "manual".into();
        settings.proxy_url = "http://127.0.0.1:7890".into();
        settings.proxy_bypass = "localhost,*.internal".into();

        let policy = proxy_policy_from_settings(&settings).unwrap();

        assert_eq!(policy.mode(), crate::network::ProxyMode::Manual);
        assert!(policy.has_proxy());
        assert!(!policy.is_direct_route());
    }

    #[test]
    fn diagnostic_proxy_policy_rejects_invalid_saved_manual_proxy() {
        let mut settings = AppSettings::default();
        settings.proxy_mode = "manual".into();

        let error = proxy_policy_from_settings(&settings).unwrap_err();

        assert_eq!(error.code, "failed");
        assert!(error.message.contains("手动代理"));
        assert!(!error.message.contains("://"));
    }

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

    #[test]
    fn imported_settings_are_bounded_and_custom_nodes_are_rebuilt() {
        let mut settings = AppSettings {
            down_concurrency: 99,
            up_concurrency: 0,
            test_duration_secs: 1,
            prometheus_port: 1,
            theme: "unknown".into(),
            node_mode: "manual".into(),
            ..Default::default()
        };
        settings.custom_nodes.push(ServerNodeInfo {
            name: "  ".into(),
            base_url: "https://example.com/".into(),
            allow_http: false,
            ..Default::default()
        });
        settings.default_node_url = "https://example.com".into();
        let normalized = normalize_imported_settings(settings).unwrap();
        assert_eq!(normalized.down_concurrency, 16);
        assert_eq!(normalized.up_concurrency, 1);
        assert_eq!(normalized.test_duration_secs, 5);
        assert_eq!(normalized.prometheus_port, 1024);
        assert_eq!(normalized.theme, "dark");
        assert_eq!(normalized.custom_nodes[0].provider, "custom");
        assert_eq!(normalized.custom_nodes[0].base_url, "https://example.com");
        assert_eq!(normalized.default_node_url, "https://example.com");
    }

    #[test]
    fn imported_settings_reject_invalid_or_duplicate_nodes() {
        let mut invalid = AppSettings::default();
        invalid.custom_nodes.push(ServerNodeInfo {
            base_url: "file:///unsafe".into(),
            ..Default::default()
        });
        assert!(normalize_imported_settings(invalid).is_err());

        let mut duplicate = AppSettings::default();
        for _ in 0..2 {
            duplicate.custom_nodes.push(ServerNodeInfo {
                base_url: "https://example.com".into(),
                ..Default::default()
            });
        }
        assert!(normalize_imported_settings(duplicate).is_err());
    }

    #[test]
    fn settings_backup_has_explicit_format_and_version() {
        let backup = SettingsBackup {
            format: SETTINGS_BACKUP_FORMAT.into(),
            schema_version: SETTINGS_BACKUP_VERSION,
            exported_at: "2026-11-16T00:00:00+08:00".into(),
            app_version: "1.13.0".into(),
            settings: AppSettings::default(),
        };
        let json = serde_json::to_string(&backup).unwrap();
        let decoded: SettingsBackup = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.format, SETTINGS_BACKUP_FORMAT);
        assert_eq!(decoded.schema_version, SETTINGS_BACKUP_VERSION);
    }

    #[test]
    fn monitor_csv_preserves_tick_and_interface_data() {
        let ticks = vec![crate::monitor::MonitorTick {
            ts_ms: 1_700_000_000_000,
            rx_bps: 1024.5,
            tx_bps: 256.25,
            rx_total: 1000,
            tx_total: 2000,
            ifaces: vec![crate::monitor::IfaceTick {
                name: "Ethernet, 1".into(),
                rx_bps: 1024.5,
                tx_bps: 256.25,
                rx_total: 1000,
                tx_total: 2000,
                up: true,
            }],
        }];
        let csv = monitor_csv(&ticks);
        assert!(csv.starts_with("timestamp_ms,rx_bps,tx_bps"));
        assert!(csv.contains("1700000000000,1024.50,256.25,1000,2000,1,"));
        assert!(csv.contains("Ethernet, 1"));
    }

    #[test]
    fn diagnostic_report_redacts_network_identifiers_but_keeps_errors() {
        let mut value = serde_json::json!({
            "hostname": "workstation",
            "local": { "mac": "00:11:22:33:44:55", "ipv4": ["192.0.2.10"] },
            "target": "example.test",
            "route_host": "router.example.test",
            "dns_domain": "example.test",
            "gateway": null,
            "error": "目标超时",
            "ok": false,
        });
        redact_diag_value(&mut value, None);
        assert_eq!(value["hostname"], "[已脱敏]");
        assert_eq!(value["local"]["mac"], "[已脱敏]");
        assert_eq!(value["local"]["ipv4"][0], "[已脱敏]");
        assert_eq!(value["target"], "[已脱敏]");
        assert_eq!(value["route_host"], "[已脱敏]");
        assert_eq!(value["dns_domain"], "[已脱敏]");
        assert!(value["gateway"].is_null());
        assert_eq!(value["error"], "目标超时");
        assert_eq!(value["ok"], false);
    }

    #[test]
    fn diagnostic_html_escapes_json() {
        let html = render_diag_html("{\"error\":\"<timeout>&\"}");
        assert!(html.contains("&lt;timeout&gt;&amp;"));
        assert!(!html.contains("<timeout>"));
    }

    #[test]
    fn batch_export_preserves_partial_and_unmeasured_targets() {
        let snapshot = crate::batchtest::BatchSnapshot {
            run_id: 1,
            started_at: "2026-11-13T12:00:00+08:00".into(),
            revision: 1,
            running: false,
            cancelled: true,
            count: 10,
            targets: vec!["localhost".into(), "queued.test".into()],
            results: vec![crate::batchtest::BatchTargetResult {
                target: "localhost".into(),
                sent: 2,
                recv: 1,
                loss_percent: 50.0,
                min_ms: Some(1.0),
                max_ms: Some(1.0),
                avg_ms: Some(1.0),
                latest_ms: None,
                samples: vec![Some(1.0), None],
            }],
        };
        let csv = batch_csv(&snapshot);
        assert!(csv.starts_with('\u{feff}'));
        assert!(csv.contains("cancelled,localhost,10,2,1,50.00"));
        assert!(csv.contains("[1.0,null]"));
        assert!(csv.contains("cancelled,queued.test,10,0,0,,,,,,[]"));
    }
}

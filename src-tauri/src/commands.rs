//! Tauri 命令层:前端唯一入口。所有业务在引擎/诊断/存储层完成,本层仅做编排与授权检查。

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
}

#[tauri::command]
pub fn get_app_info(state: tauri::State<AppState>) -> AppInfo {
    AppInfo {
        name: "NetVision Pro".into(),
        version: env!("CARGO_PKG_VERSION").into(),
        os: std::env::consts::OS.to_string(),
        data_dir: state.data_dir.display().to_string(),
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
) -> Result<(), String> {
    // 并发约束(引擎层还会二次 clamp)
    let mut s = settings;
    s.down_concurrency = s.down_concurrency.clamp(1, 16);
    s.up_concurrency = s.up_concurrency.clamp(1, 8);
    s.test_duration_secs = s.test_duration_secs.clamp(5, 30);
    s.anim_intensity = s.anim_intensity.clamp(0.0, 1.5);
    s.refresh_interval_ms = s.refresh_interval_ms.clamp(200, 5000);
    s.auto_test_interval_minutes = s.auto_test_interval_minutes.clamp(15, 1440);
    if !["dark", "light", "system"].contains(&s.theme.as_str()) {
        s.theme = "dark".into();
    }
    if !["auto", "manual"].contains(&s.node_mode.as_str()) {
        s.node_mode = "auto".into();
    }

    // 写锁贯穿"持久化 → 副作用":并发设置命令串行化(防丢更新),
    // 且保存失败时不产生任何已生效但未持久化的副作用
    let mut guard = state.settings.settings.write().await;
    state.settings.save(&s).await?;
    *guard = s.clone();

    // 开机自启动真实生效
    use tauri_plugin_autostart::ManagerExt as _;
    let autolaunch = app.autolaunch();
    if s.autostart {
        autolaunch.enable().map_err(|e| e.to_string())?;
    } else {
        autolaunch.disable().map_err(|e| e.to_string())?;
    }

    state.monitor.set_enabled(s.monitor_enabled);

    // Prometheus 端点按设置启停(apply 内部自动判重)
    {
        let storage = state.storage.clone();
        let connected_getter: Arc<dyn Fn() -> bool + Send + Sync> =
            Arc::new(|| crate::diagnostics::current_net_status().connected);
        state
            .prometheus
            .apply(s.prometheus_enabled, s.prometheus_port, storage, connected_getter);
    }
    Ok(())
}

#[tauri::command]
pub fn get_network_status() -> diagnostics::NetStatus {
    diagnostics::current_net_status()
}

// ============================================================
// 测速节点
// ============================================================

#[tauri::command]
pub async fn servers_list(state: tauri::State<'_, AppState>) -> Result<Vec<ServerNodeInfo>, String> {
    let settings = state.settings.settings.read().await;
    let favorites = state.storage.favorites_map();
    let engine = state.engine.clone();
    let custom = settings.custom_nodes.clone();
    drop(settings);
    Ok(engine
        .registry()
        .list_servers(&custom, &favorites)
        .await)
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
    // URL 安全校验(手动节点强制 HTTPS)
    util::validate_server_url(&url, false)?;
    let mut guard = state.settings.settings.write().await;
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
    state
        .engine
        .start(crate::engine::EventSink::App(app), settings, state.storage.clone())
}

#[tauri::command]
pub fn speedtest_stop(state: tauri::State<AppState>) {
    state.engine.stop();
}

#[tauri::command]
pub fn speedtest_running(state: tauri::State<AppState>) -> bool {
    state.engine.is_running()
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
) -> Result<Vec<HistoryRow>, String> {
    state.storage.list_history(from, to, desc.unwrap_or(true), limit.unwrap_or(500))
}

#[tauri::command]
pub async fn history_detail(
    state: tauri::State<'_, AppState>,
    id: i64,
) -> Result<Option<TestResult>, String> {
    state.storage.get_result_detail(id)
}

#[tauri::command]
pub async fn history_delete(state: tauri::State<'_, AppState>, id: i64) -> Result<(), String> {
    state.storage.delete_history(id)
}

#[tauri::command]
pub async fn history_clear(state: tauri::State<'_, AppState>) -> Result<(), String> {
    state.storage.clear_history()
}

/// 打开系统保存对话框并写出 CSV / JSON
#[tauri::command]
pub async fn history_export(
    app: AppHandle,
    state: tauri::State<'_, AppState>,
    format: String,
) -> Result<String, String> {
    let rows = state.storage.list_history(None, None, true, 5000)?;
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
            (json, "netvision_history.json".to_string(), vec![("JSON 文件", &["json"][..])])
        }
        _ => return Err("不支持的导出格式".into()),
    };
    let path = save_via_dialog(app, &default_name, filters).await?;
    std::fs::write(&path, content).map_err(|e| format!("写入失败: {e}"))?;
    Ok(path)
}

/// 为单条测速生成 HTML 报告并保存
#[tauri::command]
pub async fn history_export_report(
    app: AppHandle,
    state: tauri::State<'_, AppState>,
    id: i64,
) -> Result<String, String> {
    let detail = state
        .storage
        .get_result_detail(id)?
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
        builder = builder.add_filter(*name, *exts);
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
        out.push_str(&format!(
            "{},{},{},{},{},{},{},{},{},{},{},{},{},{},{}\n",
            r.created_at,
            fmt2(r.download_mbps),
            r.upload_mbps.map_or_else(String::new, |v| fmt2(v)),
            fmt2(r.idle_ping_ms),
            fmt2(r.jitter_ms),
            r.loaded_down_ms.map_or_else(String::new, |v| fmt2(v)),
            r.loaded_up_ms.map_or_else(String::new, |v| fmt2(v)),
            r.packet_loss_percent.map_or_else(String::new, |v| fmt2(v)),
            csv_escape(&r.server_name),
            csv_escape(&r.server_provider),
            csv_escape(&r.network_type),
            csv_escape(&r.interface_name),
            r.public_ip.clone().unwrap_or_default(),
            r.isp.clone().unwrap_or_default(),
            r.status,
        ));
    }
    out
}

fn fmt2(v: f64) -> String {
    format!("{v:.2}")
}

fn csv_escape(s: &str) -> String {
    if s.contains(',') || s.contains('"') || s.contains('\n') {
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
pub fn monitor_history(state: tauri::State<AppState>, window_secs: u32) -> Vec<crate::monitor::MonitorTick> {
    state.monitor.history(window_secs)
}

#[tauri::command]
pub fn monitor_latest(state: tauri::State<AppState>) -> Option<crate::monitor::MonitorTick> {
    state.monitor.latest()
}

// ============================================================
// 网络诊断
// ============================================================

#[tauri::command]
pub fn diag_local_network() -> diagnostics::LocalNetInfo {
    let ifaces = diagnostics::collect_interfaces();
    let hostname = hostname::get()
        .map(|h| h.to_string_lossy().to_string())
        .unwrap_or_default();
    diagnostics::LocalNetInfo {
        interfaces: ifaces,
        hostname,
        os: format!("{} {}", std::env::consts::OS, std::env::consts::ARCH),
    }
}

#[tauri::command]
pub async fn diag_wifi_details() -> Option<diagnostics::WifiDetails> {
    diagnostics::wifi_details().await
}

#[tauri::command]
pub async fn diag_public_info(
    state: tauri::State<'_, AppState>,
) -> Result<diagnostics::PublicInfo, String> {
    let allowed = state.settings.settings.read().await.show_public_ip;
    Ok(diagnostics::public_info(allowed).await)
}

#[tauri::command]
pub async fn diag_dns_test() -> diagnostics::DnsTestReport {
    diagnostics::dns_test().await
}

#[tauri::command]
pub async fn diag_gateway_test() -> diagnostics::GatewayReport {
    diagnostics::gateway_test().await
}

#[tauri::command]
pub async fn diag_tcp_test(
    targets: Option<Vec<String>>,
) -> Vec<diagnostics::TcpResult> {
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
    out
}

#[tauri::command]
pub async fn diag_https_test() -> Vec<diagnostics::HttpsResult> {
    let urls = [
        "https://speed.cloudflare.com/__down?bytes=0",
        "https://www.msftconnecttest.com/connecttest.txt",
        "https://www.baidu.com",
    ];
    let mut out = Vec::new();
    for u in urls {
        out.push(diagnostics::https_test(u).await);
    }
    out
}

#[tauri::command]
pub async fn diag_traceroute(
    app: AppHandle,
    host: String,
    max_hops: Option<u32>,
) -> Result<Vec<crate::engine::quality::TraceHop>, String> {
    let hops = diagnostics::traceroute(&host, max_hops.unwrap_or(15).clamp(3, 30)).await?;
    // 逐跳事件(前端可实时渲染)
    for h in &hops {
        let _ = app.emit("diag://trace", h);
    }
    Ok(hops)
}

#[tauri::command]
pub async fn diag_ping_test(host: String, count: Option<u32>) -> PingReport {
    let count = count.unwrap_or(8).clamp(2, 30);
    let out = diagnostics::icmp_ping(&host, count, 1500).await;
    PingReport {
        host,
        sent: count,
        loss_percent: out.loss_percent,
        rtts_ms: out.rtts_ms,
        avg_ms: out.avg_ms,
    }
}

#[derive(Serialize)]
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
) -> Result<diagnostics::IpVersionCheck, String> {
    let allowed = state.settings.settings.read().await.show_public_ip;
    Ok(diagnostics::ipv6_check(allowed).await)
}

#[tauri::command]
pub async fn diag_route_quality(
    host: String,
    max_hops: Option<u32>,
    probes: Option<u32>,
) -> Result<diagnostics::RouteQualityReport, String> {
    diagnostics::route_quality(&host, max_hops.unwrap_or(12), probes.unwrap_or(6)).await
}

#[tauri::command]
pub async fn diag_dns_consistency(
    domain: String,
) -> Result<diagnostics::DnsConsistencyReport, String> {
    if domain.trim().is_empty() {
        return Err("域名不能为空".into());
    }
    Ok(diagnostics::dns_consistency(&domain).await)
}

#[tauri::command]
pub async fn diag_nat_type() -> Result<diagnostics::NatReport, String> {
    Ok(diagnostics::nat_type().await)
}

#[tauri::command]
pub async fn diag_connectivity_grid() -> Result<Vec<diagnostics::ConnectivityEntry>, String> {
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
pub async fn dns_distribution(domain: String) -> Result<crate::batchtest::DnsDistributionReport, String> {
    crate::batchtest::dns_distribution(&domain).await
}

/// 快速分享文本(剪贴板)
#[tauri::command]
pub fn copy_result_text(result: crate::engine::model::TestResult) -> Result<String, String> {
    let mut s = String::new();
    s.push_str(&format!(
        "NetVision Pro 测速结果({})\n",
        result.started_at
    ));
    s.push_str(&format!("↓ 下载:{:.2} Mbps(峰值 {:.1})\n", result.download_mbps, result.download_peak_mbps));
    if let Some(up) = result.upload_mbps {
        s.push_str(&format!("↑ 上传:{:.2} Mbps(峰值 {:.1})\n", up, result.upload_peak_mbps.unwrap_or(0.0)));
    }
    s.push_str(&format!(
        "延迟:{:.1} ms / 抖动:{:.1} ms / 丢包:{}\n",
        result.idle_ping_ms,
        result.jitter_ms,
        result.packet_loss_percent.map(|l| format!("{:.1}%", l)).unwrap_or_else(|| "未知".into())
    ));
    if let Some(q) = &result.quality {
        s.push_str(&format!("质量:{}({}/100)\n", q.rating, q.score));
    }
    if let Some(srv) = &result.server {
        s.push_str(&format!("节点:{}", srv.name));
    }
    Ok(s)
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
    fn test_speedtest_service_whitelist() {
        // 已知服务均返回 HTTPS 固定入口
        for id in [
            "speedtest_cn", "ookla", "fast", "cloudflare",
            "telecom_gd", "unicom", "mobile_bj",
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

//! NetVision Pro — 专业级桌面网络测速与诊断软件
//!
//! 架构分层:
//! - engine:测速引擎(LibreSpeed/Cloudflare 适配器、节点优选、质量分析)
//! - diagnostics:网络诊断(接口/DNS/网关/TCP/HTTPS/ICMP/路由追踪)
//! - monitor:实时网络监控(1s 采样,1h 环形缓冲)
//! - storage:SQLite 历史库 + 节点收藏
//! - settings:设置管理(原子持久化)
//! - commands:Tauri 命令层(前端唯一入口)
//!
//! 安全约定:
//! - 前端不直接访问网络,所有网络 I/O 在 Rust 侧完成
//! - 日志不记录 IP / 主机名等敏感信息
//! - 公网信息仅在用户授权时获取

mod batchtest;
pub mod cli;
mod commands;
mod diagnostics;
mod engine;
mod monitor;
mod pingmon;
mod prometheus;
mod report;
mod safe_lock;
mod scheduler;
mod settings;
mod storage;

use std::sync::Arc;

use tauri::{
    menu::{Menu, MenuItem},
    tray::TrayIconBuilder,
    Manager, WindowEvent,
};

use commands::AppState;
use settings::SettingsManager;
use storage::Storage;

/// 托盘 tooltip 用的速率文本
fn fmt_rate(bps: f64) -> String {
    const UNITS: [&str; 4] = ["B/s", "KB/s", "MB/s", "GB/s"];
    let mut v = bps.max(0.0);
    let mut i = 0;
    while v >= 1024.0 && i < UNITS.len() - 1 {
        v /= 1024.0;
        i += 1;
    }
    format!("{:.1} {}", v, UNITS[i])
}

pub fn run() {
    tauri::Builder::default()
        .plugin(
            tauri_plugin_log::Builder::new()
                .level(log::LevelFilter::Info)
                .max_file_size(512_000)
                .targets([
                    tauri_plugin_log::Target::new(tauri_plugin_log::TargetKind::Stdout),
                    tauri_plugin_log::Target::new(tauri_plugin_log::TargetKind::LogDir {
                        file_name: Some("netvision".into()),
                    }),
                ])
                .build(),
        )
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            // 二次启动:唤起已有窗口
            if let Some(w) = app.get_webview_window("main") {
                let _ = w.show();
                let _ = w.set_focus();
            }
        }))
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            None,
        ))
        .setup(|app| {
            let data_dir = app.path().app_data_dir().expect("无法定位应用数据目录");
            std::fs::create_dir_all(&data_dir)?;

            let settings_mgr = SettingsManager::load(&data_dir);
            let storage = Arc::new(Storage::open(&data_dir.join("history.db"))?);
            let engine = Arc::new(engine::SpeedTestEngine::new());

            // 托盘图标(内嵌资源,避免运行时读文件失败)
            let icon =
                tauri::image::Image::from_bytes(include_bytes!("../icons/32x32.png"))?.to_owned();
            let show_item = MenuItem::with_id(app, "show", "显示主窗口", true, None::<&str>)?;
            let test_item = MenuItem::with_id(app, "start-test", "开始测速", true, None::<&str>)?;
            let quit_item =
                MenuItem::with_id(app, "quit", "退出 NetVision Pro", true, None::<&str>)?;
            let menu = Menu::with_items(app, &[&show_item, &test_item, &quit_item])?;
            let _tray = TrayIconBuilder::with_id("nv-tray")
                .icon(icon)
                .tooltip("NetVision Pro — 网络测速与诊断")
                .menu(&menu)
                .show_menu_on_left_click(false)
                .on_menu_event(|app, ev| match ev.id.as_ref() {
                    "show" => {
                        if let Some(w) = app.get_webview_window("main") {
                            let _ = w.show();
                            let _ = w.set_focus();
                        }
                    }
                    "start-test" => {
                        use crate::engine::EventSink;
                        let state = app.state::<AppState>();
                        let settings = tauri::async_runtime::block_on(async {
                            state.settings.settings.read().await.clone()
                        });
                        if let Err(e) = state.engine.start(
                            EventSink::App(app.clone()),
                            settings,
                            state.storage.clone(),
                        ) {
                            log::info!("托盘启动测速跳过: {e}");
                        }
                    }
                    "quit" => {
                        app.exit(0);
                    }
                    _ => {}
                })
                .on_tray_icon_event(|tray, ev| {
                    if let tauri::tray::TrayIconEvent::Click {
                        button: tauri::tray::MouseButton::Left,
                        button_state: tauri::tray::MouseButtonState::Up,
                        ..
                    } = ev
                    {
                        if let Some(w) = tray.app_handle().get_webview_window("main") {
                            let _ = w.show();
                            let _ = w.set_focus();
                        }
                    }
                })
                .build(app)?;

            // 后台网络监控(受设置开关控制)
            let monitor = monitor::spawn(app.handle().clone(), settings_mgr.settings.clone());
            let pingmon = Arc::new(pingmon::PingMonState::default());
            let batchtest = Arc::new(batchtest::BatchTestState::default());
            let prometheus = Arc::new(prometheus::PrometheusHandle::new());

            // 开机自启动与设置同步
            {
                let s = tauri::async_runtime::block_on(settings_mgr.settings.read());
                use tauri_plugin_autostart::ManagerExt as _;
                let al = app.autolaunch();
                if s.autostart {
                    let _ = al.enable();
                } else {
                    let _ = al.disable();
                }
            }

            app.manage(AppState {
                settings: settings_mgr,
                storage: storage.clone(),
                engine: engine.clone(),
                monitor: monitor.clone(),
                pingmon,
                prometheus,
                batchtest,
                data_dir,
            });

            // 启动时执行保留策略清理
            {
                let storage_c = storage.clone();
                let days = tauri::async_runtime::block_on(async {
                    app.state::<AppState>()
                        .settings
                        .settings
                        .read()
                        .await
                        .retention_days
                });
                if days > 0 {
                    match storage_c.cleanup_retention(days) {
                        Ok(n) if n > 0 => log::info!("保留策略:已清理 {n} 条过期记录"),
                        Ok(_) => {}
                        Err(e) => log::warn!("保留策略清理失败: {e}"),
                    }
                    // 诊断记录共用同一保留策略,避免单独再养一个设置项
                    match storage_c.cleanup_diag_runs(days) {
                        Ok(n) if n > 0 => log::info!("保留策略:已清理 {n} 条过期诊断记录"),
                        Ok(_) => {}
                        Err(e) => log::warn!("诊断记录清理失败: {e}"),
                    }
                }
            }

            // Prometheus 端点:启动时按设置自愈
            {
                let st = app.state::<AppState>();
                let (en, port) = tauri::async_runtime::block_on(async {
                    let s = st.settings.settings.read().await;
                    (s.prometheus_enabled, s.prometheus_port)
                });
                let storage_c = storage.clone();
                let connected_getter: Arc<dyn Fn() -> bool + Send + Sync> =
                    Arc::new(|| crate::diagnostics::current_net_status().connected);
                st.prometheus.apply(en, port, storage_c, connected_getter);
            }

            // 延迟/丢包阈值告警任务
            scheduler::spawn_alerts(
                app.handle().clone(),
                app.state::<AppState>().settings.settings.clone(),
                engine.clone(),
            );

            // 计划测速调度器(后台常驻,受设置开关控制)
            scheduler::spawn(
                app.handle().clone(),
                app.state::<AppState>().settings.settings.clone(),
                engine.clone(),
                storage.clone(),
                app.path().app_data_dir()?,
            );

            // 托盘 tooltip 实时速率
            {
                use std::time::Duration as StdDuration;
                let handle = app.handle().clone();
                tauri::async_runtime::spawn(async move {
                    let mut tick = tokio::time::interval(StdDuration::from_secs(2));
                    loop {
                        tick.tick().await;
                        let Some(tray) = handle.tray_by_id("nv-tray") else {
                            return;
                        };
                        let text = match handle.state::<AppState>().monitor.latest() {
                            Some(t) => format!(
                                "NetVision Pro\n↓ {} ↑ {}",
                                fmt_rate(t.rx_bps),
                                fmt_rate(t.tx_bps)
                            ),
                            None => "NetVision Pro — 网络测速与诊断".into(),
                        };
                        let _ = tray.set_tooltip(Some(text));
                    }
                });
            }
            Ok(())
        })
        .on_window_event(|window, event| {
            if let WindowEvent::CloseRequested { api, .. } = event {
                // 关闭到托盘设置真实生效;否则走完整退出流程
                let to_tray = window
                    .app_handle()
                    .try_state::<AppState>()
                    .map(|s| {
                        tauri::async_runtime::block_on(async {
                            s.settings.settings.read().await.close_to_tray
                        })
                    })
                    .unwrap_or(false);
                if to_tray {
                    api.prevent_close();
                    let _ = window.hide();
                } else {
                    // 正常退出:tauri 会自行清理 tokio 任务与托盘
                }
            }
        })
        .invoke_handler(tauri::generate_handler![
            commands::get_app_info,
            commands::get_settings,
            commands::set_settings,
            commands::get_network_status,
            commands::prometheus_status,
            commands::servers_list,
            commands::servers_refresh,
            commands::servers_set_favorite,
            commands::servers_set_manual,
            commands::servers_set_auto,
            commands::servers_add_custom,
            commands::servers_remove_custom,
            commands::speedtest_start,
            commands::speedtest_stop,
            commands::speedtest_running,
            commands::speedtest_phase,
            commands::open_speedtest_service,
            commands::history_list,
            commands::history_count,
            commands::history_detail,
            commands::history_delete,
            commands::history_clear,
            commands::history_export,
            commands::history_export_report,
            commands::monitor_set_enabled,
            commands::monitor_set_paused,
            commands::monitor_history,
            commands::monitor_latest,
            commands::monitor_paused,
            commands::diag_local_network,
            commands::diag_history_list,
            commands::diag_history_detail,
            commands::diag_history_clear,
            commands::diag_wifi_details,
            commands::diag_public_info,
            commands::diag_dns_test,
            commands::diag_gateway_test,
            commands::diag_tcp_test,
            commands::diag_https_test,
            commands::diag_traceroute,
            commands::diag_ping_test,
            commands::diag_ipv6_check,
            commands::diag_route_quality,
            commands::diag_dns_consistency,
            commands::diag_nat_type,
            commands::diag_connectivity_grid,
            commands::pingmon_start,
            commands::pingmon_stop,
            commands::pingmon_host,
            commands::copy_result_text,
            commands::batchtest_start,
            commands::batchtest_stop,
            commands::batchtest_running,
            commands::dns_distribution,
        ])
        .run(tauri::generate_context!())
        .expect("NetVision Pro 启动失败");
}

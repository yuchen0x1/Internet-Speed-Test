//! CLI 模式:复用与桌面端完全相同的测速引擎,用于脚本化与自动化场景。
//!
//! 用法:
//!   netvision-pro.exe cli speedtest [--json] [--duration 5-30] [--down 1-16] [--up 1-8]
//!                                   [--node <url>] [--precision] [--no-save]
//!
//! 输出:
//!   - 进度信息 → stderr(阶段/节点/失败原因)
//!   - 结果     → stdout(--json 为完整 JSON,否则人类可读摘要)
//! 退出码:0 完成 / 2 已取消 / 3 失败 / 1 参数错误

use std::path::PathBuf;
use std::sync::Arc;

use crate::engine::model::EngineEvent;
use crate::engine::{EventSink, SpeedTestEngine};
use crate::settings::{AppSettings, SettingsManager};
use crate::storage::Storage;

pub fn run_cli(args: &[String]) -> i32 {
    match parse_flags(args) {
        Ok(flags) => {
            let rt = tokio::runtime::Builder::new_multi_thread()
                .enable_all()
                .build()
                .expect("无法创建 tokio 运行时");
            rt.block_on(cli_main(flags))
        }
        Err(e) => {
            eprintln!("参数错误: {e}");
            print_usage();
            1
        }
    }
}

#[derive(Default)]
struct CliFlags {
    json: bool,
    duration: Option<u32>,
    down: Option<u32>,
    up: Option<u8>,
    node: Option<String>,
    precision: bool,
    no_save: bool,
}

fn print_usage() {
    eprintln!(
        "用法: netvision-pro cli speedtest [--json] [--duration 5-30] [--down 1-16] [--up 1-8] [--node <url>] [--precision] [--no-save]"
    );
}

fn parse_flags(args: &[String]) -> Result<CliFlags, String> {
    // 期望形如: cli speedtest --json ...
    if args.iter().any(|a| a == "speedtest") {
        // ok
    } else {
        return Err("缺少子命令 speedtest".into());
    }
    let mut f = CliFlags::default();
    let mut it = args.iter().skip(1).peekable();
    while let Some(a) = it.next() {
        match a.as_str() {
            "cli" | "speedtest" => {}
            "--json" => f.json = true,
            "--precision" => f.precision = true,
            "--no-save" => f.no_save = true,
            "--duration" => {
                let v = it.next().ok_or("--duration 缺少值")?;
                f.duration = Some(v.parse().map_err(|_| "--duration 需要数字(5-30)")?);
            }
            "--down" => {
                let v = it.next().ok_or("--down 缺少值")?;
                f.down = Some(v.parse().map_err(|_| "--down 需要数字(1-16)")?);
            }
            "--up" => {
                let v = it.next().ok_or("--up 缺少值")?;
                f.up = Some(v.parse().map_err(|_| "--up 需要数字(1-8)")?);
            }
            "--node" => {
                let v = it.next().ok_or("--node 缺少值")?;
                f.node = Some(v.clone());
            }
            other => return Err(format!("未知参数: {other}")),
        }
    }
    Ok(f)
}

fn data_dir() -> PathBuf {
    std::env::var("APPDATA")
        .map(|p| PathBuf::from(p).join("com.netvisionpro.app"))
        .unwrap_or_else(|_| std::env::temp_dir().join("netvision-pro-cli"))
}

async fn cli_main(flags: CliFlags) -> i32 {
    let dir = data_dir();
    let settings_mgr = SettingsManager::load(&dir);
    let mut settings: AppSettings = settings_mgr.settings.read().await.clone();
    if let Some(d) = flags.duration {
        settings.test_duration_secs = d.clamp(5, 30);
    }
    if let Some(d) = flags.down {
        settings.down_concurrency = d.clamp(1, 16);
    }
    if let Some(u) = flags.up {
        settings.up_concurrency = u.clamp(1, 8) as u32;
    }
    settings.precision_mode |= flags.precision;
    settings.autosave_history &= !flags.no_save;
    if let Some(url) = flags.node {
        // 与选择器 list_servers 的归一化一致:去掉尾斜杠,否则匹配不上
        settings.node_mode = "manual".into();
        settings.default_node_url = url.trim_end_matches('/').to_string();
    }

    let storage = match Storage::open(&dir.join("history.db")) {
        Ok(s) => Arc::new(s),
        Err(e) => {
            eprintln!("打开历史数据库失败: {e}");
            return 3;
        }
    };

    // 事件通道:后台线程把阶段/节点/失败打到 stderr
    let (tx, rx) = std::sync::mpsc::channel::<EngineEvent>();
    let reader = std::thread::spawn(move || {
        for ev in rx {
            match ev {
                EngineEvent::Phase { phase, message } => {
                    eprintln!("[{phase}] {message}");
                }
                EngineEvent::Server { node } => {
                    eprintln!("[server] {} ({})", node.name, node.provider);
                }
                EngineEvent::Failed { error } => {
                    eprintln!("[failed] {error}");
                }
                EngineEvent::Cancelled => {
                    eprintln!("[cancelled]");
                }
                _ => {}
            }
        }
    });

    eprintln!("NetVision Pro CLI v{}", env!("CARGO_PKG_VERSION"));
    let engine = Arc::new(SpeedTestEngine::new());
    let result = engine
        .run_once(EventSink::Chan(tx), settings, storage)
        .await;
    let _ = reader.join();

    if flags.json {
        match serde_json::to_string_pretty(&result) {
            Ok(s) => println!("{s}"),
            Err(e) => eprintln!("结果序列化失败: {e}"),
        }
    } else {
        println!("=== NetVision Pro 测速结果 ===");
        println!("状态      : {}", result.status);
        if let Some(err) = &result.error {
            println!("错误      : {err}");
        }
        println!("下载      : {:.2} Mbps(峰值 {:.2})", result.download_mbps, result.download_peak_mbps);
        if let Some(up) = result.upload_mbps {
            println!("上传      : {:.2} Mbps(峰值 {:.2})", up, result.upload_peak_mbps.unwrap_or(0.0));
        } else {
            println!("上传      : 未获得");
        }
        println!(
            "延迟      : {:.1} ms(最小 {:.1} / 最大 {:.1})",
            result.idle_ping_ms, result.ping_min_ms, result.ping_max_ms
        );
        println!("抖动      : {:.1} ms", result.jitter_ms);
        match result.packet_loss_percent {
            Some(l) => println!("丢包率    : {:.1} %(ICMP 实测)", l),
            None => println!("丢包率    : 未知(目标禁 ping)"),
        }
        println!("HTTP 失败 : {} 次(与丢包分开统计)", result.http_failures);
        if let (Some(d), Some(u)) = (result.loaded_down_ms, result.loaded_up_ms) {
            println!("负载延迟  : 下载 {d:.0} ms / 上传 {u:.0} ms");
        }
        if let Some(s) = &result.server {
            println!("节点      : {}", s.name);
        }
        if let Some(q) = &result.quality {
            println!("质量评级  : {}({}/100)", q.rating, q.score);
            for n in &q.notes {
                println!("  - {n}");
            }
        }
        println!("耗时      : {:.1}s", result.duration_ms as f64 / 1000.0);
    }

    match result.status.as_str() {
        "completed" => 0,
        "cancelled" => 2,
        _ => 3,
    }
}

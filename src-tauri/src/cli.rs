//! CLI 模式:复用与桌面端完全相同的测速引擎,用于脚本化与自动化场景。
//!
//! 用法:
//!   netvision-pro.exe cli speedtest [--json] [--duration 5-30] [--down 1-16] [--up 1-8]
//!                                   [--node <url>] [--precision] [--no-save]
//!   netvision-pro.exe cli history list [--json] [--from YYYY-MM-DD] [--to YYYY-MM-DD]
//!                                   [--query <text>] [--status completed|failed|cancelled]
//!                                   [--limit 1-5000] [--asc]
//!   netvision-pro.exe cli history export [--format csv|json] [--from YYYY-MM-DD]
//!                                   [--to YYYY-MM-DD] [--query <text>] [--status <status>]
//!
//! 输出:
//!   - 进度信息 → stderr(阶段/节点/失败原因)
//!   - 结果     → stdout(--json 为完整 JSON,否则人类可读摘要)
//!
//! 退出码:0 完成 / 2 已取消 / 3 失败 / 1 参数错误

use std::path::PathBuf;
use std::sync::Arc;

use crate::engine::model::EngineEvent;
use crate::engine::{EventSink, SpeedTestEngine};
use crate::settings::{AppSettings, SettingsManager};
use crate::storage::{HistoryRow, Storage};

pub fn run_cli(args: &[String]) -> i32 {
    match parse_command(args) {
        Ok(CliCommand::Speedtest(flags)) => {
            let rt = tokio::runtime::Builder::new_multi_thread()
                .enable_all()
                .build()
                .expect("无法创建 tokio 运行时");
            rt.block_on(cli_main(flags))
        }
        Ok(CliCommand::History(command)) => cli_history(command),
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

enum CliCommand {
    Speedtest(CliFlags),
    History(HistoryCommand),
}

struct HistoryCommand {
    action: HistoryAction,
    flags: HistoryFlags,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum HistoryAction {
    List,
    Export,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum HistoryFormat {
    Csv,
    Json,
}

#[derive(Default)]
struct HistoryFlags {
    json: bool,
    format: Option<HistoryFormat>,
    from: Option<String>,
    to: Option<String>,
    query: Option<String>,
    status: Option<String>,
    limit: Option<u32>,
    order_desc: bool,
}

fn print_usage() {
    eprintln!(
        "用法: netvision-pro cli speedtest [--json] [--duration 5-30] [--down 1-16] [--up 1-8] [--node <url>] [--precision] [--no-save]"
    );
    eprintln!(
        "      netvision-pro cli history list [--json] [--from YYYY-MM-DD] [--to YYYY-MM-DD] [--query <text>] [--status <status>] [--limit 1-5000] [--asc]"
    );
    eprintln!(
        "      netvision-pro cli history export [--format csv|json] [--from YYYY-MM-DD] [--to YYYY-MM-DD] [--query <text>] [--status <status>]"
    );
}

fn parse_command(args: &[String]) -> Result<CliCommand, String> {
    let mut it = args.iter().skip(1);
    match it.next().map(String::as_str) {
        Some("cli") | Some("--cli") => {}
        _ => return Err("缺少子命令 cli".into()),
    }
    match it.next().map(String::as_str) {
        Some("speedtest") => Ok(CliCommand::Speedtest(parse_speedtest_flags(it)?)),
        Some("history") => Ok(CliCommand::History(parse_history_command(it)?)),
        Some(other) => Err(format!("未知子命令: {other}")),
        None => Err("缺少子命令 speedtest 或 history".into()),
    }
}

fn parse_speedtest_flags<'a, I>(args: I) -> Result<CliFlags, String>
where
    I: Iterator<Item = &'a String>,
{
    let mut f = CliFlags::default();
    let mut it = args.peekable();
    while let Some(a) = it.next() {
        match a.as_str() {
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

fn parse_history_command<'a, I>(mut args: I) -> Result<HistoryCommand, String>
where
    I: Iterator<Item = &'a String>,
{
    let action = match args.next().map(String::as_str) {
        Some("list") => HistoryAction::List,
        Some("export") => HistoryAction::Export,
        Some(other) => return Err(format!("未知 history 子命令: {other}")),
        None => return Err("缺少 history 子命令 list 或 export".into()),
    };
    let mut flags = HistoryFlags {
        order_desc: true,
        ..HistoryFlags::default()
    };
    let mut it = args.peekable();
    while let Some(a) = it.next() {
        match a.as_str() {
            "--json" => flags.json = true,
            "--asc" => flags.order_desc = false,
            "--desc" => flags.order_desc = true,
            "--from" => flags.from = Some(next_value(&mut it, "--from")?),
            "--to" => flags.to = Some(next_value(&mut it, "--to")?),
            "--query" => flags.query = Some(next_value(&mut it, "--query")?),
            "--status" => flags.status = Some(next_value(&mut it, "--status")?),
            "--limit" => {
                let value = next_value(&mut it, "--limit")?;
                let limit = value
                    .parse::<u32>()
                    .map_err(|_| "--limit 需要数字(1-5000)".to_string())?;
                if !(1..=5000).contains(&limit) {
                    return Err("--limit 范围为 1-5000".into());
                }
                flags.limit = Some(limit);
            }
            "--format" => {
                let value = next_value(&mut it, "--format")?;
                flags.format = Some(match value.as_str() {
                    "csv" => HistoryFormat::Csv,
                    "json" => HistoryFormat::Json,
                    _ => return Err("--format 只能是 csv 或 json".into()),
                });
            }
            other => return Err(format!("未知参数: {other}")),
        }
    }
    if action == HistoryAction::List && flags.format.is_some() {
        return Err("history list 不支持 --format，请使用 --json".into());
    }
    Ok(HistoryCommand { action, flags })
}

fn next_value<'a, I>(it: &mut std::iter::Peekable<I>, flag: &str) -> Result<String, String>
where
    I: Iterator<Item = &'a String>,
{
    it.next().cloned().ok_or_else(|| format!("{flag} 缺少值"))
}

fn data_dir() -> PathBuf {
    if let Some(dir) = std::env::var_os("NETVISION_DATA_DIR").filter(|v| !v.is_empty()) {
        return PathBuf::from(dir);
    }
    #[cfg(windows)]
    if let Some(base) = std::env::var_os("APPDATA") {
        return PathBuf::from(base).join("com.netvisionpro.app");
    }
    #[cfg(target_os = "macos")]
    if let Some(home) = std::env::var_os("HOME") {
        return PathBuf::from(home).join("Library/Application Support/com.netvisionpro.app");
    }
    #[cfg(target_os = "linux")]
    {
        if let Some(base) =
            std::env::var_os("XDG_DATA_HOME").filter(|p| PathBuf::from(p).is_absolute())
        {
            return PathBuf::from(base).join("com.netvisionpro.app");
        }
        if let Some(home) = std::env::var_os("HOME") {
            return PathBuf::from(home).join(".local/share/com.netvisionpro.app");
        }
    }
    // No shared temporary directory: fail visibly when no persistent home exists.
    PathBuf::from(".netvision-pro")
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
    let output_settings = settings.clone();
    let result = engine
        .run_once(EventSink::Chan(tx), settings, storage)
        .await;
    let _ = reader.join();

    if let Some(warning) = &result.history_save_error {
        eprintln!("[history] {warning}");
    }

    if flags.json {
        match cli_result_json(&result, &output_settings) {
            Ok(s) => println!("{s}"),
            Err(e) => eprintln!("结果序列化失败: {e}"),
        }
    } else {
        println!("=== NetVision Pro 测速结果 ===");
        println!("状态      : {}", result.status);
        if let Some(err) = &result.error {
            println!("错误      : {err}");
        }
        println!(
            "下载      : {:.2} Mbps(峰值 {:.2})",
            result.download_mbps, result.download_peak_mbps
        );
        if let Some(up) = result.upload_mbps {
            println!(
                "上传      : {:.2} Mbps(峰值 {:.2})",
                up,
                result.upload_peak_mbps.unwrap_or(0.0)
            );
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

fn cli_result_json(
    result: &crate::engine::model::TestResult,
    settings: &AppSettings,
) -> Result<String, serde_json::Error> {
    let mut value = serde_json::to_value(result)?;
    redact_cli_urls(&mut value);
    if let Some(object) = value.as_object_mut() {
        let node = result.server.as_ref();
        object.insert(
            "experiment".into(),
            serde_json::json!({
                "schema_version": 1,
                "app_version": env!("CARGO_PKG_VERSION"),
                "command": "cli speedtest",
                "protocol": node.map(|s| s.provider.as_str()),
                "node_id": node.map(|s| s.id.as_str()),
                "node_name": node.map(|s| s.name.as_str()),
                "duration_secs": settings.test_duration_secs,
                "down_concurrency": settings.down_concurrency,
                "up_concurrency": settings.up_concurrency,
                "precision_mode": settings.precision_mode,
                "node_mode": settings.node_mode,
                "autosave_history": settings.autosave_history,
            }),
        );
    }
    serde_json::to_string_pretty(&value)
}

fn redact_cli_urls(value: &mut serde_json::Value) {
    match value {
        serde_json::Value::Object(map) => {
            for (key, child) in map.iter_mut() {
                let lower = key.to_ascii_lowercase();
                if (lower == "base_url" || lower.ends_with("_url")) && child.is_string() {
                    *child = serde_json::Value::String("[已脱敏]".into());
                } else {
                    redact_cli_urls(child);
                }
            }
        }
        serde_json::Value::Array(items) => {
            for item in items {
                redact_cli_urls(item);
            }
        }
        _ => {}
    }
}

fn cli_history(command: HistoryCommand) -> i32 {
    let dir = data_dir();
    let storage = match Storage::open(&dir.join("history.db")) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("打开历史数据库失败: {e}");
            return 3;
        }
    };
    let limit = command.flags.limit.unwrap_or(match command.action {
        HistoryAction::List => 100,
        HistoryAction::Export => 5000,
    });
    let rows = match storage.list_history_filtered(
        command.flags.from.clone(),
        command.flags.to.clone(),
        command.flags.query.clone(),
        command.flags.status.clone(),
        command.flags.order_desc,
        limit,
    ) {
        Ok(rows) => rows,
        Err(e) => {
            eprintln!("读取历史失败: {e}");
            return 1;
        }
    };
    if command.action == HistoryAction::Export {
        let format = command.flags.format.unwrap_or(if command.flags.json {
            HistoryFormat::Json
        } else {
            HistoryFormat::Csv
        });
        match format {
            HistoryFormat::Json => match serde_json::to_string_pretty(&rows) {
                Ok(s) => println!("{s}"),
                Err(e) => {
                    eprintln!("历史序列化失败: {e}");
                    return 1;
                }
            },
            HistoryFormat::Csv => print!("{}", history_csv(&rows)),
        }
        return 0;
    }
    if command.flags.json {
        match serde_json::to_string_pretty(&rows) {
            Ok(s) => println!("{s}"),
            Err(e) => {
                eprintln!("历史序列化失败: {e}");
                return 1;
            }
        }
    } else {
        print_history_table(&rows);
    }
    0
}

fn print_history_table(rows: &[HistoryRow]) {
    println!("ID\t时间\t状态\t下载 Mbps\t上传 Mbps\t延迟 ms\t节点");
    for row in rows {
        println!(
            "{}\t{}\t{}\t{:.2}\t{}\t{:.1}\t{}",
            row.id,
            row.created_at,
            row.status,
            row.download_mbps,
            row.upload_mbps
                .map(|v| format!("{v:.2}"))
                .unwrap_or_else(|| "未知".into()),
            row.idle_ping_ms,
            row.server_name,
        );
    }
}

fn csv_escape(value: &str) -> String {
    let value = neutralize_formula(value);
    if value.contains([',', '"', '\n', '\r']) {
        format!("\"{}\"", value.replace('"', "\"\""))
    } else {
        value
    }
}

fn neutralize_formula(value: &str) -> String {
    let starts_with_formula = matches!(
        value.trim_start().chars().next(),
        Some('=' | '+' | '-' | '@')
    );
    let starts_with_control = value
        .as_bytes()
        .first()
        .is_some_and(|byte| *byte == b'\t' || *byte == b'\r' || *byte == b'\n');
    if starts_with_formula || starts_with_control {
        format!("'{value}")
    } else {
        value.to_string()
    }
}

fn history_csv(rows: &[HistoryRow]) -> String {
    let mut out = String::from(
        "id,created_at,status,download_mbps,upload_mbps,idle_ping_ms,jitter_ms,loaded_down_ms,loaded_up_ms,packet_loss_percent,server_name,server_provider,server_url,network_type,interface_name,public_ip,isp\n",
    );
    for row in rows {
        let values = [
            row.id.to_string(),
            row.created_at.clone(),
            row.status.clone(),
            row.download_mbps.to_string(),
            row.upload_mbps.map(|v| v.to_string()).unwrap_or_default(),
            row.idle_ping_ms.to_string(),
            row.jitter_ms.to_string(),
            row.loaded_down_ms
                .map(|v| v.to_string())
                .unwrap_or_default(),
            row.loaded_up_ms.map(|v| v.to_string()).unwrap_or_default(),
            row.packet_loss_percent
                .map(|v| v.to_string())
                .unwrap_or_default(),
            row.server_name.clone(),
            row.server_provider.clone(),
            row.server_url.clone(),
            row.network_type.clone(),
            row.interface_name.clone(),
            row.public_ip.clone().unwrap_or_default(),
            row.isp.clone().unwrap_or_default(),
        ];
        out.push_str(
            &values
                .iter()
                .map(|value| csv_escape(value))
                .collect::<Vec<_>>()
                .join(","),
        );
        out.push('\n');
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_history_export_json_filters() {
        let args = vec![
            "netvision-pro".into(),
            "cli".into(),
            "history".into(),
            "export".into(),
            "--format".into(),
            "json".into(),
            "--from".into(),
            "2026-11-01".into(),
            "--status".into(),
            "failed".into(),
            "--query".into(),
            "alpha".into(),
        ];
        let CliCommand::History(command) = parse_command(&args).unwrap() else {
            panic!("expected history command")
        };
        assert_eq!(command.action, HistoryAction::Export);
        assert_eq!(command.flags.format, Some(HistoryFormat::Json));
        assert_eq!(command.flags.from.as_deref(), Some("2026-11-01"));
        assert_eq!(command.flags.status.as_deref(), Some("failed"));
        assert_eq!(command.flags.query.as_deref(), Some("alpha"));
    }

    #[test]
    fn rejects_history_list_format_and_bad_limit() {
        let list = vec![
            "netvision-pro".into(),
            "cli".into(),
            "history".into(),
            "list".into(),
            "--format".into(),
            "csv".into(),
        ];
        assert!(parse_command(&list).is_err());
        let bad_limit = vec![
            "netvision-pro".into(),
            "cli".into(),
            "history".into(),
            "list".into(),
            "--limit".into(),
            "5001".into(),
        ];
        assert!(parse_command(&bad_limit).is_err());
    }

    #[test]
    fn csv_escapes_formula_and_delimiters() {
        let mut row = HistoryRow {
            id: 1,
            created_at: "2026-11-16 10:00:00".into(),
            download_mbps: 1.0,
            upload_mbps: None,
            idle_ping_ms: 2.0,
            jitter_ms: 0.1,
            loaded_down_ms: None,
            loaded_up_ms: None,
            packet_loss_percent: None,
            server_name: "=SUM(A1)".into(),
            server_provider: "provider, one".into(),
            server_url: "https://example.test".into(),
            network_type: "wifi".into(),
            interface_name: "Wi-Fi".into(),
            public_ip: None,
            isp: Some("ISP \"A\"".into()),
            status: "completed".into(),
        };
        let csv = history_csv(&[row.clone()]);
        assert!(csv.contains("\"provider, one\""));
        assert!(csv.contains("\"ISP \"\"A\"\"\""));
        assert!(csv.contains("'=SUM(A1)"));
        row.server_name = "line\nbreak".into();
        assert!(history_csv(&[row]).contains("\"line\nbreak\""));
    }

    #[test]
    fn cli_json_metadata_does_not_include_node_url() {
        let settings = AppSettings {
            node_mode: "manual".into(),
            default_node_url: "https://example.test?access_token=secret".into(),
            ..AppSettings::default()
        };
        let json = cli_result_json(&Default::default(), &settings).unwrap();
        assert!(json.contains("schema_version"));
        assert!(!json.contains("access_token"));
        assert!(!json.contains("secret"));
    }

    #[test]
    fn cli_json_redacts_urls_inside_result_server() {
        let settings = AppSettings::default();
        let result = crate::engine::model::TestResult {
            server: Some(crate::engine::model::ServerNodeInfo {
                provider: "custom".into(),
                base_url: "https://example.test?access_token=secret".into(),
                ..Default::default()
            }),
            ..Default::default()
        };
        let json = cli_result_json(&result, &settings).unwrap();
        assert!(!json.contains("access_token"));
        assert!(!json.contains("secret"));
        assert!(json.contains("[已脱敏]"));
    }
}

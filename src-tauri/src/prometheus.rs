//! Prometheus / Grafana 指标导出:极简 HTTP/1.1 端点(仅 GET /metrics)。
//!
//! - 仅绑定 127.0.0.1(默认),不对外网暴露
//! - 指标来自最近一次测速结果与当前网络状态,均为真实数据

use std::sync::Arc;

use serde::Serialize;

use crate::storage::Storage;

/// 构造 Prometheus 文本格式(纯函数,可单测)
pub fn build_metrics_text(
    latest: Option<&crate::storage::HistoryRow>,
    tests_total: u64,
    connected: bool,
) -> String {
    let mut s = String::new();
    s.push_str("# HELP netvision_tests_total Stored speed test records (all statuses; decreases on deletion)\n");
    s.push_str("# TYPE netvision_tests_total gauge\n");
    s.push_str(&format!("netvision_tests_total {tests_total}\n"));
    s.push_str("# HELP netvision_network_connected Network currently connected\n");
    s.push_str("# TYPE netvision_network_connected gauge\n");
    s.push_str(&format!(
        "netvision_network_connected {}\n",
        if connected { 1 } else { 0 }
    ));
    if let Some(r) = latest.filter(|r| r.status == "completed") {
        s.push_str("# HELP netvision_last_download_mbps Last download speed (Mbps)\n");
        s.push_str("# TYPE netvision_last_download_mbps gauge\n");
        s.push_str(&format!(
            "netvision_last_download_mbps {:.2}\n",
            r.download_mbps
        ));
        s.push_str("# HELP netvision_last_upload_mbps Last upload speed (Mbps)\n");
        s.push_str("# TYPE netvision_last_upload_mbps gauge\n");
        if let Some(up) = r.upload_mbps {
            s.push_str(&format!("netvision_last_upload_mbps {up:.2}\n"));
        }
        s.push_str("# HELP netvision_last_ping_ms Last idle ping (ms)\n");
        s.push_str("# TYPE netvision_last_ping_ms gauge\n");
        s.push_str(&format!("netvision_last_ping_ms {:.2}\n", r.idle_ping_ms));
        s.push_str("# HELP netvision_last_jitter_ms Last jitter (ms)\n");
        s.push_str("# TYPE netvision_last_jitter_ms gauge\n");
        s.push_str(&format!("netvision_last_jitter_ms {:.2}\n", r.jitter_ms));
        if let Some(loss) = r.packet_loss_percent {
            s.push_str("# HELP netvision_last_loss_percent Last ICMP packet loss (%)\n");
            s.push_str("# TYPE netvision_last_loss_percent gauge\n");
            s.push_str(&format!("netvision_last_loss_percent {loss:.2}\n"));
        }
        s.push_str("# HELP netvision_last_test_timestamp Last test Unix timestamp (seconds)\n");
        s.push_str("# TYPE netvision_last_test_timestamp gauge\n");
        if let Some(ts) = local_timestamp(&r.created_at, &chrono::Local) {
            s.push_str(&format!("netvision_last_test_timestamp {ts}\n"));
        }
    }
    s
}

fn local_timestamp<T: chrono::TimeZone>(value: &str, zone: &T) -> Option<i64> {
    // Newer offset-bearing records are unambiguous; legacy records use local time.
    if let Ok(ts) = chrono::DateTime::parse_from_rfc3339(value) {
        return Some(ts.timestamp());
    }
    let ts = chrono::NaiveDateTime::parse_from_str(value, "%Y-%m-%d %H:%M:%S%.f").ok()?;
    // Omit ambiguous/nonexistent DST times rather than inventing an instant.
    zone.from_local_datetime(&ts)
        .single()
        .map(|ts| ts.timestamp())
}

fn scrape_text(snapshot: &crate::storage::HistoryMetrics, connected: bool) -> String {
    let mut text = build_metrics_text(snapshot.latest.as_ref(), snapshot.total as u64, connected);
    for (status, count) in [
        ("completed", snapshot.completed),
        ("failed", snapshot.failed),
        ("cancelled", snapshot.cancelled),
    ] {
        text.push_str(&format!("# HELP netvision_history_{status} Stored {status} speed tests\n# TYPE netvision_history_{status} gauge\nnetvision_history_{status} {count}\n"));
    }
    text
}

fn request_status(header: &str) -> u16 {
    let mut parts = header.lines().next().unwrap_or_default().split_whitespace();
    let (Some(method), Some(target), Some(version)) = (parts.next(), parts.next(), parts.next())
    else {
        return 400;
    };
    if parts.next().is_some() || !matches!(version, "HTTP/1.0" | "HTTP/1.1") {
        return 400;
    }
    if method != "GET" {
        return 405;
    }
    if target.split('?').next() != Some("/metrics") {
        return 404;
    }
    200
}

async fn respond(sock: &mut tokio::net::TcpStream, status: u16, body: &str) {
    use tokio::io::AsyncWriteExt;
    let reason = match status {
        200 => "OK",
        400 => "Bad Request",
        404 => "Not Found",
        405 => "Method Not Allowed",
        408 => "Request Timeout",
        431 => "Request Header Fields Too Large",
        _ => "Service Unavailable",
    };
    let allow = if status == 405 { "Allow: GET\r\n" } else { "" };
    let response = format!("HTTP/1.1 {status} {reason}\r\nContent-Type: text/plain; version=0.0.4; charset=utf-8\r\nContent-Length: {}\r\n{allow}Connection: close\r\n\r\n{body}", body.len());
    let _ = sock.write_all(response.as_bytes()).await;
    let _ = sock.shutdown().await;
}

async fn serve(
    mut sock: tokio::net::TcpStream,
    storage: Arc<Storage>,
    connected: Arc<dyn Fn() -> bool + Send + Sync>,
) {
    use tokio::io::AsyncReadExt;
    let header = tokio::time::timeout(std::time::Duration::from_secs(2), async {
        let mut header = Vec::new();
        let mut chunk = [0u8; 1024];
        loop {
            let n = sock.read(&mut chunk).await.map_err(|_| 400u16)?;
            if n == 0 {
                return Err(400u16);
            }
            header.extend_from_slice(&chunk[..n]);
            if header.len() > 8192 {
                return Err(431u16);
            }
            if header.windows(4).any(|bytes| bytes == b"\r\n\r\n") {
                return String::from_utf8(header).map_err(|_| 400u16);
            }
        }
    })
    .await;
    let status = match &header {
        Ok(Ok(header)) => request_status(header),
        Ok(Err(code)) => *code,
        Err(_) => 408,
    };
    if status != 200 {
        respond(&mut sock, status, "Invalid metrics request\n").await;
        return;
    }
    // Storage failure must fail the scrape, never fabricate an empty database.
    match tokio::task::spawn_blocking(move || storage.metrics_snapshot()).await {
        Ok(Ok(snapshot)) => respond(&mut sock, 200, &scrape_text(&snapshot, connected())).await,
        _ => respond(&mut sock, 503, "Metrics storage unavailable\n").await,
    }
}

struct PromInner {
    enabled: bool,
    port: u32,
    token: Option<tokio_util::sync::CancellationToken>,
    /// 真实监听状态:true 表示已成功 bind 并进入 accept 循环。
    /// 与设置里的 enabled 分开,才能区分"用户想开"和"真的开起来了"。
    listening: bool,
    /// 最近一次绑定失败的原因(绑定成功后清空)
    last_error: Option<String>,
}

/// 前端可见的端点真实状态(绑定失败时设置页不能显示"已开启")
#[derive(Debug, Clone, Serialize)]
pub struct PrometheusStatus {
    /// 用户设置是否要求开启
    pub requested: bool,
    /// 是否真的在监听
    pub listening: bool,
    /// 实际(或尝试)绑定的地址
    pub url: String,
    /// 绑定失败原因;正常时为 None
    pub error: Option<String>,
}

pub struct PrometheusHandle {
    /// 状态与启停判定在同一把锁内完成,避免并发 apply 产生孤儿监听端口
    inner: Arc<std::sync::Mutex<PromInner>>,
}

impl PrometheusHandle {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(std::sync::Mutex::new(PromInner {
                enabled: false,
                port: 9876,
                token: None,
                listening: false,
                last_error: None,
            })),
        }
    }

    /// 返回端点真实状态(供 UI 展示;不改变任何运行态)
    pub fn status(&self) -> PrometheusStatus {
        let inner = crate::safe_lock::lock(&self.inner);
        PrometheusStatus {
            requested: inner.enabled,
            listening: inner.listening,
            url: format!("http://127.0.0.1:{}/metrics", inner.port),
            error: inner.last_error.clone(),
        }
    }

    /// 按设置启停(端口或开关变化时安全重启)
    pub fn apply(
        &self,
        enabled: bool,
        port: u32,
        storage: Arc<Storage>,
        connected_getter: Arc<dyn Fn() -> bool + Send + Sync>,
    ) {
        let mut inner = crate::safe_lock::lock(&self.inner);
        if inner.enabled == enabled && inner.port == port && inner.token.is_some() == enabled {
            return; // 无变化
        }
        // 先停(取消在锁内完成,与绑定失败回滚的判定互斥)
        if let Some(t) = inner.token.take() {
            t.cancel();
        }
        inner.enabled = enabled;
        inner.port = port;
        inner.listening = false;
        inner.last_error = None;
        if !enabled {
            return;
        }
        let token = tokio_util::sync::CancellationToken::new();
        inner.token = Some(token.clone());
        let slot = self.inner.clone();
        tauri::async_runtime::spawn(async move {
            let addr = format!("127.0.0.1:{port}");
            let listener = match tokio::net::TcpListener::bind(&addr).await {
                Ok(l) => l,
                Err(e) => {
                    log::warn!("Prometheus 端口 {addr} 绑定失败: {e}");
                    // 回滚:未真正监听时不保留"假活"状态,下次 apply 可重试绑定。
                    // 同时把原因记入 last_error,设置页据此显示真实失败信息。
                    let mut g = crate::safe_lock::lock(&slot);
                    if !token.is_cancelled() {
                        g.listening = false;
                        g.token = None;
                        g.last_error = Some(format!("端口 {port} 绑定失败: {e}"));
                    }
                    return;
                }
            };
            // 已真正进入监听:标记 listening 并清掉历史错误
            {
                let mut g = crate::safe_lock::lock(&slot);
                if token.is_cancelled() {
                    return;
                }
                g.listening = true;
                g.last_error = None;
            }
            log::info!("Prometheus 指标端点已启动: http://{addr}/metrics");
            let slots = Arc::new(tokio::sync::Semaphore::new(16));
            loop {
                tokio::select! {
                    _ = token.cancelled() => break,
                    accepted = listener.accept() => {
                        let Ok((sock, _)) = accepted else { continue };
                        // Drop excess sockets instead of spawning an unbounded queue.
                        let Ok(permit) = slots.clone().try_acquire_owned() else { continue };
                        let storage = storage.clone();
                        let connected = connected_getter.clone();
                        tauri::async_runtime::spawn(async move {
                            let _permit = permit;
                            let _ = tokio::time::timeout(std::time::Duration::from_secs(5), serve(sock, storage, connected)).await;
                        });
                    }
                }
            }
            // accept 循环退出(被取消或 listener 异常):不再处于监听态
            {
                let mut g = crate::safe_lock::lock(&slot);
                if !token.is_cancelled() {
                    g.listening = false;
                }
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::HistoryRow;

    #[test]
    fn test_metrics_text_with_row() {
        let row = HistoryRow {
            id: 1,
            created_at: "2026-11-07 12:14:56".into(),
            download_mbps: 183.1,
            upload_mbps: Some(51.1),
            idle_ping_ms: 178.99,
            jitter_ms: 0.62,
            loaded_down_ms: Some(180.0),
            loaded_up_ms: Some(52.0),
            packet_loss_percent: Some(0.0),
            server_name: "Tokyo".into(),
            server_provider: "librespeed".into(),
            server_url: "https://x".into(),
            network_type: "ethernet".into(),
            interface_name: "以太网".into(),
            public_ip: None,
            isp: None,
            status: "completed".into(),
        };
        let text = build_metrics_text(Some(&row), 5, true);
        assert!(text.contains("netvision_tests_total 5"));
        assert!(text.contains("netvision_last_download_mbps 183.10"));
        assert!(text.contains("netvision_last_upload_mbps 51.10"));
        assert!(text.contains("netvision_last_ping_ms 178.99"));
        assert!(text.contains("netvision_last_jitter_ms 0.62"));
        assert!(text.contains("netvision_last_loss_percent 0.00"));
        assert!(text.contains("netvision_network_connected 1"));
        assert!(text.contains("netvision_last_test_timestamp "));
    }

    #[test]
    fn test_metrics_text_empty() {
        let text = build_metrics_text(None, 0, false);
        assert!(text.contains("netvision_tests_total 0"));
        assert!(text.contains("netvision_network_connected 0"));
        assert!(!text.contains("netvision_last_download_mbps"));
    }
    #[test]
    fn timestamp_respects_local_offset_and_unknown_values() {
        let zone = chrono::FixedOffset::east_opt(8 * 3600).unwrap();
        let expected = chrono::DateTime::parse_from_rfc3339("2026-11-13T12:00:00+08:00")
            .unwrap()
            .timestamp();
        assert_eq!(
            local_timestamp("2026-11-13 12:00:00", &zone),
            Some(expected)
        );
        assert_eq!(
            local_timestamp("2026-11-13T12:00:00+08:00", &zone),
            Some(expected)
        );
        assert_eq!(local_timestamp("invalid", &zone), None);
    }

    #[test]
    fn routes_match_exact_path_and_method() {
        for (line, status) in [
            ("GET /metrics HTTP/1.1", 200),
            ("GET /metrics?x=1 HTTP/1.0", 200),
            ("GET /metrics-evil HTTP/1.1", 404),
            ("POST /metrics HTTP/1.1", 405),
            ("GET /metrics garbage", 400),
            ("GET", 400),
        ] {
            assert_eq!(request_status(line), status);
        }
    }

    #[tokio::test]
    async fn endpoint_handles_fragmented_http_and_storage_failure() {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        // Exercise real TCP without touching the application database.
        for broken in [false, true] {
            let storage = Arc::new(Storage::open(std::path::Path::new(":memory:")).unwrap());
            if broken {
                storage.invalidate_metrics_storage_for_test();
            }
            let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
            let address = listener.local_addr().unwrap();
            let server = tokio::spawn(async move {
                let (sock, _) = listener.accept().await.unwrap();
                serve(sock, storage, Arc::new(|| true)).await;
            });
            let mut client = tokio::net::TcpStream::connect(address).await.unwrap();
            client.write_all(b"GET /met").await.unwrap();
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
            client
                .write_all(b"rics HTTP/1.1\r\nHost: localhost\r\n\r\n")
                .await
                .unwrap();
            let mut response = String::new();
            client.read_to_string(&mut response).await.unwrap();
            assert!(response.starts_with(if broken {
                "HTTP/1.1 503"
            } else {
                "HTTP/1.1 200"
            }));
            if !broken {
                assert!(response.contains("netvision_history_completed 0"));
            }
            server.await.unwrap();
        }
    }
}

//! Prometheus / Grafana 指标导出:极简 HTTP/1.1 端点(仅 GET /metrics)。
//!
//! - 仅绑定 127.0.0.1(默认),不对外网暴露
//! - 指标来自最近一次测速结果与当前网络状态,均为真实数据

use std::sync::Arc;

use crate::storage::Storage;

/// 构造 Prometheus 文本格式(纯函数,可单测)
pub fn build_metrics_text(latest: Option<&crate::storage::HistoryRow>, tests_total: u64, connected: bool) -> String {
    let mut s = String::new();
    s.push_str("# HELP netvision_tests_total Total completed speed tests\n");
    s.push_str("# TYPE netvision_tests_total counter\n");
    s.push_str(&format!("netvision_tests_total {tests_total}\n"));
    s.push_str("# HELP netvision_network_connected Network currently connected\n");
    s.push_str("# TYPE netvision_network_connected gauge\n");
    s.push_str(&format!("netvision_network_connected {}\n", if connected { 1 } else { 0 }));
    if let Some(r) = latest {
        s.push_str("# HELP netvision_last_download_mbps Last download speed (Mbps)\n");
        s.push_str("# TYPE netvision_last_download_mbps gauge\n");
        s.push_str(&format!("netvision_last_download_mbps {:.2}\n", r.download_mbps));
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
        // created_at 为本地时间字符串,转时间戳
        if let Ok(ts) = chrono::NaiveDateTime::parse_from_str(&r.created_at, "%Y-%m-%d %H:%M:%S") {
            s.push_str(&format!(
                "netvision_last_test_timestamp {}\n",
                ts.and_utc().timestamp()
            ));
        }
    }
    s
}

struct PromInner {
    enabled: bool,
    port: u32,
    token: Option<tokio_util::sync::CancellationToken>,
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
            })),
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
        let mut inner = self.inner.lock().unwrap();
        if inner.enabled == enabled && inner.port == port && inner.token.is_some() == enabled {
            return; // 无变化
        }
        // 先停(取消在锁内完成,与绑定失败回滚的判定互斥)
        if let Some(t) = inner.token.take() {
            t.cancel();
        }
        inner.enabled = enabled;
        inner.port = port;
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
                    // 回滚:未真正监听时不保留"假活"状态,下次 apply 可重试绑定
                    let mut g = slot.lock().unwrap();
                    if !token.is_cancelled() {
                        g.enabled = false;
                        g.token = None;
                    }
                    return;
                }
            };
            log::info!("Prometheus 指标端点已启动: http://{addr}/metrics");
            loop {
                tokio::select! {
                    _ = token.cancelled() => return,
                    accepted = listener.accept() => {
                        let Ok((mut sock, _)) = accepted else { continue };
                        let storage = storage.clone();
                        let connected = connected_getter.clone();
                        tauri::async_runtime::spawn(async move {
                            use tokio::io::{AsyncReadExt, AsyncWriteExt};
                            let mut buf = [0u8; 1024];
                            // 读请求头(一小段即可)
                            let _ = tokio::time::timeout(
                                std::time::Duration::from_secs(2),
                                sock.read(&mut buf),
                            ).await;
                            let req = String::from_utf8_lossy(&buf);
                            let body = if req.starts_with("GET /metrics") {
                                // 查询放阻塞线程池:抓取路径不占用 tokio 工作线程
                                let (latest, total) = {
                                    let storage = storage.clone();
                                    tokio::task::spawn_blocking(move || {
                                        let latest = storage
                                            .list_history(None, None, true, 1)
                                            .ok()
                                            .and_then(|v| v.first().cloned());
                                        let total =
                                            storage.history_count().unwrap_or(0) as u64;
                                        (latest, total)
                                    })
                                    .await
                                    .unwrap_or((None, 0))
                                };
                                build_metrics_text(latest.as_ref(), total, connected())
                            } else {
                                String::new()
                            };
                            let resp = format!(
                                "HTTP/1.1 200 OK\r\nContent-Type: text/plain; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                                body.len()
                            );
                            let _ = sock.write_all(resp.as_bytes()).await;
                            let _ = sock.shutdown().await;
                        });
                    }
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
}

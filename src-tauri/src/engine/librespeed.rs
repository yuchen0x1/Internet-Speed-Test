//! LibreSpeed 协议适配器(兼容官方与社区 LibreSpeed 服务器)。
//!
//! 协议端点:
//! - ping:  GET  {base}/empty.php
//! - 下载:  GET  {base}/garbage.php?ckSize=N(服务器返回约 N MB 随机数据)
//! - 上传:  POST {base}/empty.php(任意大小请求体)
//! - IP:    GET  {base}/getIP.php

use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;

use tokio_util::sync::CancellationToken;

use super::adapter::{
    cache_buster, measure_http_rtt, PhaseConfig, ProviderAdapter, ThroughputCounters,
};
use super::model::ServerNodeInfo;

/// 上传分块大小边界
const UPLOAD_CHUNK_MIN: usize = 128 * 1024;
const UPLOAD_CHUNK_INITIAL: usize = 2 * 1024 * 1024;
const UPLOAD_CHUNK_MAX: usize = 4 * 1024 * 1024;
/// 每次计数粒度(流式计数字节块)
const COUNT_GRANULARITY: usize = 64 * 1024;

pub struct LibreSpeedAdapter {
    client: reqwest::Client,
}

impl LibreSpeedAdapter {
    pub fn new() -> Self {
        let client = reqwest::Client::builder()
            .user_agent(format!("NetVisionPro/{}", env!("CARGO_PKG_VERSION")))
            .connect_timeout(Duration::from_secs(8))
            .pool_idle_timeout(Duration::from_secs(30))
            .no_proxy()
            .build()
            .expect("构建 HTTP 客户端失败");
        Self { client }
    }
}

#[async_trait]
impl ProviderAdapter for LibreSpeedAdapter {
    fn provider(&self) -> &'static str {
        "librespeed"
    }

    fn client(&self) -> &reqwest::Client {
        &self.client
    }

    async fn ping_rtt(
        &self,
        node: &ServerNodeInfo,
        token: &CancellationToken,
    ) -> Result<f64, String> {
        measure_http_rtt(&self.client, &node.ping_endpoint(), token).await
    }

    async fn run_download(
        &self,
        node: &ServerNodeInfo,
        counters: Arc<ThroughputCounters>,
        cfg: PhaseConfig,
        token: CancellationToken,
    ) {
        let dl = node.download_url();
        let deadline = tokio::time::Instant::now() + cfg.duration;
        let mut set = tokio::task::JoinSet::new();
        for _ in 0..cfg.streams.max(1) {
            let client = self.client.clone();
            let dl = dl.clone();
            let counters = counters.clone();
            let token = token.clone();
            set.spawn(async move {
                // 持续请求 garbage.php 直到阶段截止,统计真实流式字节数
                while !token.is_cancelled() && tokio::time::Instant::now() < deadline {
                    let url = format!("{}?ckSize=100&_x={}", dl, cache_buster());
                    let resp = match client
                        .get(&url)
                        .header("Cache-Control", "no-cache")
                        .timeout(Duration::from_secs(20))
                        .send()
                        .await
                    {
                        Ok(r) if r.status().is_success() => r,
                        Ok(_) => {
                            counters.add_failure();
                            // 短暂退避后重试
                            tokio::time::sleep(Duration::from_millis(300)).await;
                            continue;
                        }
                        Err(_) => {
                            counters.add_failure();
                            tokio::time::sleep(Duration::from_millis(300)).await;
                            continue;
                        }
                    };
                    let mut stream = resp.bytes_stream();
                    futures_util::pin_mut!(stream);
                    while let Some(item) = tok::select_chunk(&mut stream, &token).await {
                        match item {
                            Ok(bytes) => counters.add_bytes(bytes.len() as u64),
                            Err(_) => {
                                counters.add_failure();
                                break;
                            }
                        }
                    }
                }
            });
        }
        // 阶段截止:取消全部流
        let _ = tokio::time::timeout_at(deadline + Duration::from_millis(500), async {
            while set.join_next().await.is_some() {}
        })
        .await;
        token.cancel();
    }

    async fn run_upload(
        &self,
        node: &ServerNodeInfo,
        counters: Arc<ThroughputCounters>,
        cfg: PhaseConfig,
        token: CancellationToken,
    ) {
        let ul = node.upload_url();
        let deadline = tokio::time::Instant::now() + cfg.duration;
        let pool = Arc::new(super::util::random_block(1024 * 1024)); // 1MB 随机池,循环复用
        let mut set = tokio::task::JoinSet::new();
        for _ in 0..cfg.streams.max(1) {
            let client = self.client.clone();
            let ul = ul.clone();
            let counters = counters.clone();
            let token = token.clone();
            let pool = pool.clone();
            set.spawn(async move {
                let mut chunk = UPLOAD_CHUNK_INITIAL;
                while !token.is_cancelled() && tokio::time::Instant::now() < deadline {
                    let body = super::upload_stream::ChunkedBody::new(
                        (*pool).clone(),
                        chunk,
                        counters.clone(),
                        token.clone(),
                    );
                    let req = client
                        .post(&ul)
                        .header("Content-Type", "application/octet-stream")
                        .timeout(Duration::from_secs(30))
                        .body(reqwest::Body::wrap_stream(body));
                    match req.send().await {
                        Ok(r) if r.status().as_u16() == 413 => {
                            // 服务器拒绝过大请求体:减半重试
                            chunk /= 2;
                            if chunk < UPLOAD_CHUNK_MIN {
                                break;
                            }
                        }
                        Ok(r) if r.status().is_success() => {
                            // 流水线化:响应体极小,后台排空即可;立即开始下一块,
                            // 消除"等服务器响应"造成的块间空隙(HTTP/2 多路复用 / 连接池复用)
                            tokio::spawn(async move {
                                let _ = r.bytes().await;
                            });
                            chunk = (chunk * 3 / 2).min(UPLOAD_CHUNK_MAX);
                        }
                        Ok(_) => {
                            counters.add_failure();
                            tokio::time::sleep(Duration::from_millis(250)).await;
                        }
                        Err(_) => {
                            counters.add_failure();
                            tokio::time::sleep(Duration::from_millis(250)).await;
                        }
                    }
                }
            });
        }
        let _ = tokio::time::timeout_at(deadline + Duration::from_millis(500), async {
            while set.join_next().await.is_some() {}
        })
        .await;
        token.cancel();
    }

    async fn fetch_ip_info(
        &self,
        node: &ServerNodeInfo,
        token: &CancellationToken,
    ) -> Option<(String, Option<String>)> {
        let url = node.ip_endpoint();
        let resp = token
            .run_until_cancelled(self.client.get(&url).timeout(Duration::from_secs(6)).send())
            .await?
            .ok()?;
        let text = token.run_until_cancelled(resp.text()).await?.ok()?;
        // getIP.php 返回 JSON:{"processedString":"Client IP: x.x.x.x (ISP 名称)", ...}
        let v: serde_json::Value = serde_json::from_str(&text).ok()?;
        let processed = v.get("processedString")?.as_str()?.to_string();
        let processed = processed
            .trim_start_matches("Client IP:")
            .trim()
            .to_string();
        // 形如 "1.2.3.4 (Some ISP)" 或纯 IP
        if let (Some(a), Some(b)) = (processed.find('('), processed.rfind(')')) {
            if a < b {
                let ip = processed[..a].trim().to_string();
                let isp = processed[a + 1..b].trim().to_string();
                return Some((ip, Some(isp)));
            }
        }
        Some((processed, None))
    }
}

/// 辅助:在等待下一个流块的同时响应取消
mod tok {
    use futures_util::StreamExt;
    use tokio_util::sync::CancellationToken;

    pub(super) async fn select_chunk<S>(
        stream: &mut std::pin::Pin<&mut S>,
        token: &CancellationToken,
    ) -> Option<Result<bytes::Bytes, reqwest::Error>>
    where
        S: futures_util::Stream<Item = Result<bytes::Bytes, reqwest::Error>>,
    {
        let mut pinned = std::pin::Pin::as_mut(stream);
        tokio::select! {
            _ = token.cancelled() => None,
            item = pinned.next() => item,
        }
    }
}

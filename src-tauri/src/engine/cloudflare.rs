//! Cloudflare 速度测速适配器(全球边缘网络,大带宽,适合高倍速线路)。
//!
//! 端点:
//! - ping:  GET https://speed.cloudflare.com/__down?bytes=0
//! - 下载:  GET https://speed.cloudflare.com/__down?bytes=N(需浏览器式 Origin/Referer)
//! - 上传:  POST https://speed.cloudflare.com/__up
//! - IP:    GET https://speed.cloudflare.com/cdn-cgi/trace(ip/colo/loc)

use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use futures_util::StreamExt;
use tokio_util::sync::CancellationToken;

use crate::network::ProxyPolicy;

use super::adapter::{
    cache_buster, measure_http_rtt, PhaseConfig, ProviderAdapter, ThroughputCounters,
};
use super::model::ServerNodeInfo;

const UPLOAD_CHUNK_INITIAL: usize = 8 * 1024 * 1024;
const UPLOAD_CHUNK_MIN: usize = 256 * 1024;
const UPLOAD_CHUNK_MAX: usize = 16 * 1024 * 1024;
/// 单次下载请求块(25MB,过大将触发服务器防护 403)
const DL_CHUNK_BYTES: u64 = 25_000_000;

pub struct CloudflareAdapter {
    client: reqwest::Client,
}

impl CloudflareAdapter {
    pub fn new() -> Self {
        Self::with_policy(&ProxyPolicy::default())
    }

    pub fn with_policy(policy: &ProxyPolicy) -> Self {
        let client = policy
            .client_builder(
                &format!(
                    "Mozilla/5.0 (Windows NT 10.0; Win64; x64) NetVisionPro/{}",
                    env!("CARGO_PKG_VERSION")
                ),
                Duration::from_secs(8),
            )
            .expect("代理策略不可用")
            .default_headers({
                let mut h = reqwest::header::HeaderMap::new();
                h.insert(
                    reqwest::header::ORIGIN,
                    reqwest::header::HeaderValue::from_static("https://speed.cloudflare.com"),
                );
                h.insert(
                    reqwest::header::REFERER,
                    reqwest::header::HeaderValue::from_static("https://speed.cloudflare.com/"),
                );
                h
            })
            .build()
            .expect("构建 Cloudflare HTTP 客户端失败");
        Self { client }
    }
}

impl Default for CloudflareAdapter {
    fn default() -> Self {
        Self::new()
    }
}

pub const CLOUDFLARE_BASE: &str = "https://speed.cloudflare.com";

/// 构造 Cloudflare 内置节点
pub fn builtin_node() -> ServerNodeInfo {
    ServerNodeInfo {
        id: super::util::node_id(CLOUDFLARE_BASE, "Cloudflare 全球边缘网络"),
        name: "Cloudflare 全球边缘网络".into(),
        provider: super::model::PROVIDER_CLOUDFLARE.into(),
        base_url: CLOUDFLARE_BASE.into(),
        dl_url: "__down".into(),
        ul_url: "__up".into(),
        ping_url: "__down".into(),
        getip_url: "cdn-cgi/trace".into(),
        location: Some("全球边缘节点".into()),
        favorite: false,
        alive: None,
        latency_ms: None,
        allow_http: false,
    }
}

#[async_trait]
impl ProviderAdapter for CloudflareAdapter {
    fn provider(&self) -> &'static str {
        super::model::PROVIDER_CLOUDFLARE
    }

    async fn ping_rtt(
        &self,
        _node: &ServerNodeInfo,
        token: &CancellationToken,
    ) -> Result<f64, String> {
        measure_http_rtt(
            &self.client,
            &format!("{CLOUDFLARE_BASE}/__down?bytes=0"),
            token,
        )
        .await
    }

    async fn run_download(
        &self,
        _node: &ServerNodeInfo,
        counters: Arc<ThroughputCounters>,
        cfg: PhaseConfig,
        token: CancellationToken,
    ) {
        let deadline = tokio::time::Instant::now() + cfg.duration;
        let mut set = tokio::task::JoinSet::new();
        for _ in 0..cfg.streams.max(1) {
            let client = self.client.clone();
            let counters = counters.clone();
            let token = token.clone();
            set.spawn(async move {
                while !token.is_cancelled() && tokio::time::Instant::now() < deadline {
                    let url = format!(
                        "{CLOUDFLARE_BASE}/__down?bytes={DL_CHUNK_BYTES}&_x={}",
                        cache_buster()
                    );
                    let resp = match token
                        .run_until_cancelled(
                            client.get(&url).timeout(Duration::from_secs(30)).send(),
                        )
                        .await
                    {
                        Some(Ok(r)) if r.status().is_success() => r,
                        Some(Ok(r)) => {
                            counters.add_failure();
                            let _ = token.run_until_cancelled(r.bytes()).await;
                            tokio::time::sleep(Duration::from_millis(300)).await;
                            continue;
                        }
                        Some(Err(_)) => {
                            counters.add_failure();
                            tokio::time::sleep(Duration::from_millis(300)).await;
                            continue;
                        }
                        None => break,
                    };
                    let stream = resp.bytes_stream();
                    futures_util::pin_mut!(stream);
                    let mut pinned = std::pin::Pin::as_mut(&mut stream);
                    while let Some(item) = tokio::select! {
                        _ = token.cancelled() => None,
                        item = pinned.next() => item,
                    } {
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
        let _ = tokio::time::timeout_at(deadline + Duration::from_millis(500), async {
            while set.join_next().await.is_some() {}
        })
        .await;
        token.cancel();
    }

    async fn run_upload(
        &self,
        _node: &ServerNodeInfo,
        counters: Arc<ThroughputCounters>,
        cfg: PhaseConfig,
        token: CancellationToken,
    ) {
        let ul = format!("{CLOUDFLARE_BASE}/__up");
        let deadline = tokio::time::Instant::now() + cfg.duration;
        let pool = Arc::new(super::util::random_block(1024 * 1024));
        let mut set = tokio::task::JoinSet::new();
        for _ in 0..cfg.streams.max(1) {
            let client = self.client.clone();
            let ul = ul.clone();
            let counters = counters.clone();
            let token = token.clone();
            let pool = pool.clone();
            set.spawn(async move {
                let mut drains = tokio::task::JoinSet::new();
                let mut chunk = UPLOAD_CHUNK_INITIAL;
                while !token.is_cancelled() && tokio::time::Instant::now() < deadline {
                    let body = super::upload_stream::ChunkedBody::new_untracked(
                        (*pool).clone(),
                        chunk,
                        counters.clone(),
                        token.clone(),
                    );
                    let produced = body.produced_bytes();
                    let req = client
                        .post(&ul)
                        .header("Content-Type", "text/plain;charset=UTF-8")
                        .header(reqwest::header::CONTENT_LENGTH, chunk.to_string())
                        .timeout(Duration::from_secs(40))
                        .body(reqwest::Body::wrap_stream(body));
                    match token.run_until_cancelled(req.send()).await {
                        None => break,
                        Some(Ok(r)) if r.status().as_u16() == 413 => {
                            counters.add_failure();
                            chunk /= 2;
                            if chunk < UPLOAD_CHUNK_MIN {
                                break;
                            }
                        }
                        Some(Ok(r)) if r.status().is_success() => {
                            if !super::upload_stream::commit_completed_upload(
                                &counters, &produced, chunk,
                            ) {
                                break;
                            }
                            // 流水线化:后台排空响应,立即开始下一块(消除块间空隙)
                            let drain_token = token.clone();
                            drains.spawn(async move {
                                tokio::select! {
                                    _ = drain_token.cancelled() => {},
                                    _ = tokio::time::timeout(Duration::from_secs(2), r.bytes()) => {},
                                }
                            });
                            chunk = (chunk * 3 / 2).min(UPLOAD_CHUNK_MAX);
                        }
                        Some(Ok(r)) => {
                            counters.add_failure();
                            let _ = token.run_until_cancelled(r.bytes()).await;
                            tokio::time::sleep(Duration::from_millis(250)).await;
                        }
                        Some(Err(_)) => {
                            counters.add_failure();
                            tokio::time::sleep(Duration::from_millis(250)).await;
                        }
                    }
                }
                while drains.join_next().await.is_some() {}
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
        _node: &ServerNodeInfo,
        token: &CancellationToken,
    ) -> Option<(String, Option<String>)> {
        let resp = token
            .run_until_cancelled(
                self.client
                    .get(format!("{CLOUDFLARE_BASE}/cdn-cgi/trace"))
                    .timeout(Duration::from_secs(6))
                    .send(),
            )
            .await?
            .ok()?;
        let text = token.run_until_cancelled(resp.text()).await?.ok()?;
        // trace 返回 key=value 行
        let mut ip = None;
        for line in text.lines() {
            if let Some(v) = line.strip_prefix("ip=") {
                ip = Some(v.trim().to_string());
                break;
            }
        }
        ip.map(|i| (i, None))
    }
}

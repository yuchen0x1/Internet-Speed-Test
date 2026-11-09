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

use super::adapter::{cache_buster, measure_http_rtt, PhaseConfig, ProviderAdapter, ThroughputCounters};
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
        let client = reqwest::Client::builder()
            .user_agent(format!(
                "Mozilla/5.0 (Windows NT 10.0; Win64; x64) NetVisionPro/{}",
                env!("CARGO_PKG_VERSION")
            ))
            .connect_timeout(Duration::from_secs(8))
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
            .no_proxy()
            .build()
            .expect("构建 Cloudflare HTTP 客户端失败");
        Self { client }
    }
}

pub const CLOUDFLARE_BASE: &str = "https://speed.cloudflare.com";

/// 构造 Cloudflare 内置节点
pub fn builtin_node() -> ServerNodeInfo {
    ServerNodeInfo {
        id: super::util::node_id(CLOUDFLARE_BASE, "Cloudflare 全球边缘网络"),
        name: "Cloudflare 全球边缘网络".into(),
        provider: "cloudflare".into(),
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
        "cloudflare"
    }

    fn client(&self) -> &reqwest::Client {
        &self.client
    }

    async fn ping_rtt(
        &self,
        _node: &ServerNodeInfo,
        token: &CancellationToken,
    ) -> Result<f64, String> {
        measure_http_rtt(&self.client, &format!("{CLOUDFLARE_BASE}/__down?bytes=0"), token).await
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
                    let resp = match client.get(&url).timeout(Duration::from_secs(30)).send().await {
                        Ok(r) if r.status().is_success() => r,
                        _ => {
                            counters.add_failure();
                            tokio::time::sleep(Duration::from_millis(300)).await;
                            continue;
                        }
                    };
                    let mut stream = resp.bytes_stream();
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
                        .header("Content-Type", "text/plain;charset=UTF-8")
                        .timeout(Duration::from_secs(40))
                        .body(reqwest::Body::wrap_stream(body));
                    match req.send().await {
                        Ok(r) if r.status().as_u16() == 413 => {
                            chunk /= 2;
                            if chunk < UPLOAD_CHUNK_MIN {
                                break;
                            }
                        }
                        Ok(r) if r.status().is_success() => {
                            // 流水线化:后台排空响应,立即开始下一块(消除块间空隙)
                            tokio::spawn(async move {
                                let _ = r.bytes().await;
                            });
                            chunk = (chunk * 3 / 2).min(UPLOAD_CHUNK_MAX);
                        }
                        _ => {
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

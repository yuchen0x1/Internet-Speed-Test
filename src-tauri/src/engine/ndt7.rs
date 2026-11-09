//! M-Lab NDT7 适配器(开源协议,无 ToS 风险)。
//!
//! 协议要点(见 m-lab/ndt-server spec/ndt7-protocol.md):
//! - 客户端以子协议 `net.measurementlab.ndt.v7` 连接 `wss://<server>/ndt/v7/download`(下载)
//!   或 `/ndt/v7/upload`(上传);服务端定期下发 JSON 测量报文,约 10-15s 后关闭连接。
//! - 服务器地址由 M-Lab Locate API v2 发现:`https://locate.measurementlab.net/v2/nearest/ndt/ndt7`,
//!   响应中 `urls` 字段给出可直接使用的绝对 URL(可能带访问令牌)。
//! - 吞吐量以本端真实收发字节计数(与其他适配器同一口径:200ms 采样)。
//! - 不提供公网 IP / ISP 接口(NDT7 规范不保证返回客户端地址),如实留空。

use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use futures_util::{SinkExt, StreamExt as _};
use tokio_tungstenite::tungstenite::client::IntoClientRequest;
use tokio_tungstenite::tungstenite::handshake::client::generate_key;
use tokio_tungstenite::tungstenite::http::header::{
    HeaderValue, SEC_WEBSOCKET_KEY, SEC_WEBSOCKET_PROTOCOL, SEC_WEBSOCKET_VERSION,
};
use tokio_tungstenite::tungstenite::http::{HeaderMap, Request};
use tokio_tungstenite::tungstenite::Message;
use tokio_util::sync::CancellationToken;

use super::adapter::{PhaseConfig, ProviderAdapter, ThroughputCounters};
use super::model::ServerNodeInfo;

/// NDT7 WebSocket 子协议(固定值)
pub const NDT7_SUBPROTOCOL: &str = "net.measurementlab.ndt.v7";
/// M-Lab Locate API v2(就近 NDT7 服务发现)
const LOCATE_URL: &str = "https://locate.measurementlab.net/v2/nearest/ndt/ndt7";
/// 单条 WS 报文上限(协议建议 ≤65535 字节)
const WS_CHUNK_BYTES: usize = 65_535;
/// 上传随机数据池大小
const UPLOAD_POOL_BYTES: usize = 1024 * 1024;

pub struct Ndt7Adapter {
    client: reqwest::Client,
    /// 已发现的服务器(去抖:避免在 Locate API 失败时反复请求)
    cached: std::sync::Mutex<Option<ServerNodeInfo>>,
}

impl Ndt7Adapter {
    pub fn new() -> Self {
        let client = reqwest::Client::builder()
            .user_agent(format!("NetVisionPro/{}", env!("CARGO_PKG_VERSION")))
            .connect_timeout(Duration::from_secs(6))
            .timeout(Duration::from_secs(10))
            .no_proxy()
            .build()
            .expect("构建 NDT7 HTTP 客户端失败");
        Self {
            client,
            cached: std::sync::Mutex::new(None),
        }
    }

    /// 通过 Locate API v2 发现 NDT7 服务。
    /// 取响应中的前若干候选,**逐个做 WS 握手探测**,选第一个真正可用的服务
    /// (实测发现 Locate 首台可能是不可达的社区服务器,不能假定可用);
    /// 全部不可用返回 None,由上层按其他协议降级。
    pub async fn discover(&self) -> Option<ServerNodeInfo> {
        if let Some(n) = self.cached.lock().unwrap().clone() {
            return Some(n);
        }
        let node = self.discover_uncached().await;
        *self.cached.lock().unwrap() = node.clone();
        node
    }

    async fn discover_uncached(&self) -> Option<ServerNodeInfo> {
        let resp = self.client.get(LOCATE_URL).send().await.ok()?;
        if !resp.status().is_success() {
            return None;
        }
        let text = resp.text().await.ok()?;
        let candidates = parse_locate_candidates(&text);
        let token = tokio_util::sync::CancellationToken::new();
        // 逐个握手探测(4s 上限),选**握手最快**的候选:Locate 首台不保证质量最优
        let mut best: Option<(ServerNodeInfo, f64)> = None;
        for c in candidates.into_iter().take(4) {
            let ws = match to_ws_url(&c.download_url()) {
                Some(u) => u,
                None => continue,
            };
            let t0 = std::time::Instant::now();
            let probe = tokio::time::timeout(Duration::from_secs(4), ws_connect(&ws, &token)).await;
            if let Ok(Ok(_stream)) = probe {
                let rtt = t0.elapsed().as_secs_f64() * 1000.0;
                let better = best.as_ref().map_or(true, |(_, b)| rtt < *b);
                if better {
                    best = Some((c, rtt));
                }
            }
        }
        best.map(|(n, _)| n)
    }
}

impl Default for Ndt7Adapter {
    fn default() -> Self {
        Self::new()
    }
}

/// 解析 Locate API v2 响应,**列出全部可用候选**(单台服务的构造见 `node_from_urls`)。
/// 结构:`{"results":[{"urls":{"wss:///ndt/v7/download":"wss://host/...", ...}}]}` 或裸数组。
pub fn parse_locate_candidates(text: &str) -> Vec<ServerNodeInfo> {
    let Ok(v) = serde_json::from_str::<serde_json::Value>(text) else {
        return Vec::new();
    };
    let servers: &Vec<serde_json::Value> = if let Some(arr) = v.as_array() {
        arr
    } else if let Some(arr) = v.get("results").and_then(|r| r.as_array()) {
        arr
    } else {
        return Vec::new();
    };
    servers
        .iter()
        .filter_map(|s| node_from_urls(s.get("urls")?.as_object()?))
        .collect()
}

/// 取第一台可用的 NDT7 服务(不验证可达性;仅用于解析单测)。
pub fn parse_locate_response(text: &str) -> Option<ServerNodeInfo> {
    parse_locate_candidates(text).into_iter().next()
}

/// 由 `urls` 映射构造 NDT7 节点。
/// 安全策略:**优先 wss://(加密)**,仅当该服务不提供 wss 时退回 ws:// 并以
/// `allow_http` 标记提示"未加密"(节点页橙色 HTTP 标记),绝不静默降级。
/// URL 可能带访问令牌,必须原样保留。
fn node_from_urls(urls: &serde_json::Map<String, serde_json::Value>) -> Option<ServerNodeInfo> {
    let mut best: Option<(String, String)> = None; // (download, upload)
    let mut plain: Option<(String, String)> = None;
    for wanted_secure in [true, false] {
        let mut dl: Option<String> = None;
        let mut ul: Option<String> = None;
        for (k, val) in urls {
            let Some(u) = val.as_str() else { continue };
            let is_secure = u.starts_with("wss://") || u.starts_with("https://");
            if is_secure != wanted_secure {
                continue;
            }
            if k.contains("ndt/v7/download") && dl.is_none() {
                dl = Some(u.to_string());
            } else if k.contains("ndt/v7/upload") && ul.is_none() {
                ul = Some(u.to_string());
            }
        }
        if let (Some(d), Some(u)) = (dl, ul) {
            if wanted_secure {
                best = Some((d, u));
                break;
            }
            plain = Some((d, u));
        }
    }
    let (dl, ul) = best.or(plain)?;
    // base_url 用主机(去掉协议与查询令牌),用于节点展示与 ID 稳定性;
    // 完整 URL(含令牌)存放在 dl_url/ul_url,来自 Locate API 的权威响应。
    let base = base_from_service_url(&dl);
    let host = super::util::extract_host(&base).unwrap_or_else(|| "mlab".into());
    let insecure = !(dl.starts_with("wss://") || dl.starts_with("https://"));
    let name = if insecure {
        format!("M-Lab NDT7 ({host}) · ws")
    } else {
        format!("M-Lab NDT7 ({host})")
    };
    let mut node = ServerNodeInfo::default();
    node.id = super::util::node_id(&base, &name);
    node.name = name;
    node.provider = super::model::PROVIDER_NDT7.into();
    node.base_url = base;
    node.dl_url = dl;
    node.ul_url = ul;
    node.ping_url = node.dl_url.clone();
    node.getip_url = String::new();
    node.location = Some("全球公共测量平台".into());
    node.favorite = false;
    node.alive = None;
    node.latency_ms = None;
    // ws://(明文)节点如实标记为"未加密",节点页显示 HTTP 标记
    node.allow_http = insecure;
    Some(node)
}

/// 从服务 URL 提取主机 + `/ndt/v7` 前缀(去掉协议与查询参数)
fn base_from_service_url(url: &str) -> String {
    let after = url
        .strip_prefix("wss://")
        .or_else(|| url.strip_prefix("ws://"))
        .or_else(|| url.strip_prefix("https://"))
        .or_else(|| url.strip_prefix("http://"))
        .unwrap_or(url);
    let rest = after.split(['?', '#']).next().unwrap_or(after);
    match rest.find('/') {
        Some(i) => format!("https://{}", &rest[..i]),
        None => format!("https://{rest}"),
    }
}

/// 归一化为 ws/wss 连接 URL:
/// - `wss://` / `ws://` 原样保留(含令牌的路径与查询不动)
/// - `https://` → `wss://`,`http://` → `ws://`
pub fn to_ws_url(url: &str) -> Option<String> {
    if url.starts_with("wss://") || url.starts_with("ws://") {
        return Some(url.to_string());
    }
    let rest = url
        .strip_prefix("https://")
        .or_else(|| url.strip_prefix("http://"))?;
    Some(format!("wss://{rest}"))
}

/// 构造带 NDT7 子协议头的 WS 握手请求
fn ws_request(url: &str) -> Result<Request<()>, String> {
    let mut req = url
        .into_client_request()
        .map_err(|_| "NDT7 URL 无效".to_string())?;
    let headers: &mut HeaderMap = req.headers_mut();
    headers.insert(
        SEC_WEBSOCKET_PROTOCOL,
        HeaderValue::from_static(NDT7_SUBPROTOCOL),
    );
    headers.insert(SEC_WEBSOCKET_VERSION, HeaderValue::from_static("13"));
    headers.insert(
        SEC_WEBSOCKET_KEY,
        HeaderValue::from_str(&generate_key()).unwrap(),
    );
    Ok(req)
}

async fn ws_connect(
    url: &str,
    token: &CancellationToken,
) -> Result<
    tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>,
    String,
> {
    let req = ws_request(url)?;
    let fut = tokio_tungstenite::connect_async(req);
    match token.run_until_cancelled(fut).await {
        Some(Ok((stream, _resp))) => Ok(stream),
        Some(Err(_)) => Err("WS 握手失败".into()),
        None => Err("已取消".into()),
    }
}

#[async_trait]
impl ProviderAdapter for Ndt7Adapter {
    fn provider(&self) -> &'static str {
        "ndt7"
    }

    fn client(&self) -> &reqwest::Client {
        &self.client
    }

    /// 以 WS 握手耗时作为延迟(NDT7 无独立 ping 端点,握手即真实连接建立过程)。
    /// 20s 上限:实测存在握手长时间挂起的社区服务,不能让单次探测拖住整个阶段。
    async fn ping_rtt(
        &self,
        node: &ServerNodeInfo,
        token: &CancellationToken,
    ) -> Result<f64, String> {
        let ws = to_ws_url(&node.download_url()).ok_or("NDT7 节点 URL 无效")?;
        let t0 = std::time::Instant::now();
        let stream = tokio::time::timeout(Duration::from_secs(20), ws_connect(&ws, token))
            .await
            .map_err(|_| "WS 握手超时".to_string())??;
        drop(stream); // 握手完成即测得往返,立即释放连接
        Ok(t0.elapsed().as_secs_f64() * 1000.0)
    }

    async fn run_download(
        &self,
        node: &ServerNodeInfo,
        counters: Arc<ThroughputCounters>,
        cfg: PhaseConfig,
        token: CancellationToken,
    ) {
        let url = match to_ws_url(&node.download_url()) {
            Some(u) => u,
            None => return,
        };
        let deadline = tokio::time::Instant::now() + cfg.duration;
        let mut set = tokio::task::JoinSet::new();
        for _ in 0..cfg.streams.max(1) {
            let url = url.clone();
            let counters = counters.clone();
            let token = token.clone();
            set.spawn(async move {
                while !token.is_cancelled() && tokio::time::Instant::now() < deadline {
                    let mut ws = match ws_connect(&url, &token).await {
                        Ok(s) => s,
                        Err(_) => {
                            tokio::time::sleep(Duration::from_millis(300)).await;
                            continue;
                        }
                    };
                    // 单条连接:服务端约 10-15s 主动关闭;循环到阶段截止再重连
                    loop {
                        if token.is_cancelled() || tokio::time::Instant::now() >= deadline {
                            return;
                        }
                        let left = deadline.saturating_duration_since(tokio::time::Instant::now());
                        let msg = tokio::time::timeout(left, ws.next()).await;
                        match msg {
                            Ok(Some(Ok(Message::Binary(b)))) => counters.add_bytes(b.len() as u64),
                            Ok(Some(Ok(Message::Text(_)))) => {
                                // NDT7 JSON telemetry 是控制报文,不计入二进制吞吐数据。
                            }
                            Ok(Some(Ok(_))) => {}
                            Ok(Some(Err(_))) => {
                                // WebSocket 传输错误后重连;HTTP 失败计数仅用于 HTTP 适配器。
                                break;
                            }
                            Ok(None) => {
                                // 服务端会主动结束测量连接,正常关闭后重连不算传输失败。
                                break;
                            }
                            Err(_) => return, // 阶段截止
                        }
                    }
                }
            });
        }
        let _ = tokio::time::timeout_at(deadline + Duration::from_millis(800), async {
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
        let url = match to_ws_url(&node.upload_url()) {
            Some(u) => u,
            None => return,
        };
        let deadline = tokio::time::Instant::now() + cfg.duration;
        let pool = Arc::new(super::util::random_block(UPLOAD_POOL_BYTES));
        let mut set = tokio::task::JoinSet::new();
        for _ in 0..cfg.streams.max(1) {
            let url = url.clone();
            let counters = counters.clone();
            let token = token.clone();
            let pool = pool.clone();
            set.spawn(async move {
                while !token.is_cancelled() && tokio::time::Instant::now() < deadline {
                    let mut ws = match ws_connect(&url, &token).await {
                        Ok(s) => s,
                        Err(_) => {
                            tokio::time::sleep(Duration::from_millis(300)).await;
                            continue;
                        }
                    };
                    let mut off = 0usize;
                    loop {
                        if token.is_cancelled() || tokio::time::Instant::now() >= deadline {
                            return;
                        }
                        let end = (off + WS_CHUNK_BYTES).min(pool.len());
                        let chunk = pool[off..end].to_vec();
                        off = if end >= pool.len() { 0 } else { end };
                        match ws.send(Message::Binary(chunk.clone())).await {
                            Ok(()) => counters.add_bytes(chunk.len() as u64),
                            Err(_) => break, // 传输错误不归入 HTTP 请求失败。
                        }
                    }
                }
            });
        }
        let _ = tokio::time::timeout_at(deadline + Duration::from_millis(800), async {
            while set.join_next().await.is_some() {}
        })
        .await;
        token.cancel();
    }

    /// NDT7 规范不保证返回客户端公网地址:如实留空,不猜测
    async fn fetch_ip_info(
        &self,
        _node: &ServerNodeInfo,
        _token: &CancellationToken,
    ) -> Option<(String, Option<String>)> {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Locate API v2 真实响应结构(含访问令牌)
    const LOCATE_FIXTURE: &str = r#"[{
      "city": "Los Angeles", "country": "US",
      "hostname": "mlab2-lax05.measurement-lab.org",
      "machine": "mlab2-lax05.mlab-oti.measurement-lab.org",
      "site": "lax05", "urlinfo": "",
      "urls": {
        "ws:///ndt/v7/download": "ws://mlab2-lax05.measurement-lab.org/ndt/v7/download?access_token=abc123",
        "ws:///ndt/v7/upload": "ws://mlab2-lax05.measurement-lab.org/ndt/v7/upload?access_token=abc123",
        "wss:///ndt/v7/download": "wss://mlab2-lax05.measurement-lab.org/ndt/v7/download?access_token=abc123",
        "wss:///ndt/v7/upload": "wss://mlab2-lax05.measurement-lab.org/ndt/v7/upload?access_token=abc123"
      },
      "ip": ["38.104.110.24"], "fqdn": "mlab2-lax05.measurement-lab.org",
      "locator": "mlab2-lax05.measurement-lab.org"
    }]"#;

    #[test]
    fn test_parse_locate_fixture_prefers_wss() {
        let n = parse_locate_response(LOCATE_FIXTURE).expect("应解析出节点");
        assert_eq!(n.provider, "ndt7");
        assert_eq!(n.base_url, "https://mlab2-lax05.measurement-lab.org");
        assert_eq!(n.name, "M-Lab NDT7 (mlab2-lax05.measurement-lab.org)");
        // 优先 wss:令牌必须原样保留
        assert_eq!(
            n.dl_url,
            "wss://mlab2-lax05.measurement-lab.org/ndt/v7/download?access_token=abc123"
        );
        assert_eq!(
            n.ul_url,
            "wss://mlab2-lax05.measurement-lab.org/ndt/v7/upload?access_token=abc123"
        );
        assert!(n
            .download_url()
            .starts_with("wss://mlab2-lax05.measurement-lab.org/ndt/v7/download"));
        assert!(n
            .upload_url()
            .ends_with("/ndt/v7/upload?access_token=abc123"));
        assert!(n.getip_url.is_empty(), "NDT7 不提供 IP 接口");
        assert!(!n.allow_http, "wss 节点不应标记为未加密");
    }

    /// 真实 Locate v2 响应结构:`{"results":[...]}`,同时给 ws 与 wss
    #[test]
    fn test_parse_locate_real_structure_prefers_wss() {
        let fixture = r#"{"results":[{
          "location": {"city":"Hong Kong","country":"HK"},
          "urls": {
            "ws:///ndt/v7/download": "ws://h.example/ndt/v7/download?access_token=w1",
            "ws:///ndt/v7/upload": "ws://h.example/ndt/v7/upload?access_token=w1",
            "wss:///ndt/v7/download": "wss://h.example/ndt/v7/download?access_token=s1",
            "wss:///ndt/v7/upload": "wss://h.example/ndt/v7/upload?access_token=s1"
          }}]}"#;
        let n = parse_locate_response(fixture).expect("应解析出节点");
        assert!(n.dl_url.starts_with("wss://"), "必须优先选择加密通道");
        assert_eq!(n.name, "M-Lab NDT7 (h.example)");
        assert!(!n.allow_http);
    }

    /// 仅有 ws://(明文)时必须如实标记未加密,而不是当作安全连接
    #[test]
    fn test_parse_locate_plain_ws_marked_insecure() {
        let fixture = r#"{"results":[{
          "location": {"city":"Nowhere","country":"XX"},
          "urls": {
            "ws:///ndt/v7/download": "ws://p.example/ndt/v7/download?t=1",
            "ws:///ndt/v7/upload": "ws://p.example/ndt/v7/upload?t=1"
          }}]}"#;
        let n = parse_locate_response(fixture).expect("应解析出节点");
        assert!(n.name.ends_with("· ws"), "明文节点名称应如实标注");
        assert!(n.allow_http, "明文节点必须标记未加密(节点页显示 HTTP 标记)");
        assert_eq!(n.base_url, "https://p.example");
    }

    #[test]
    fn test_to_ws_url_variants() {
        assert_eq!(
            to_ws_url("https://h.example/ndt/v7/download?access_token=x").as_deref(),
            Some("wss://h.example/ndt/v7/download?access_token=x")
        );
        assert_eq!(
            to_ws_url("wss://h.example/x?t=1").as_deref(),
            Some("wss://h.example/x?t=1")
        );
        assert_eq!(
            to_ws_url("ws://h.example/x").as_deref(),
            Some("ws://h.example/x")
        );
        assert_eq!(to_ws_url("ftp://x"), None);
    }

    #[test]
    fn test_base_from_service_url() {
        assert_eq!(
            base_from_service_url("https://h.example/a/b?token=1"),
            "https://h.example"
        );
        assert_eq!(
            base_from_service_url("https://h.example"),
            "https://h.example"
        );
        assert_eq!(base_from_service_url("h.example/x"), "https://h.example");
    }

    #[test]
    fn test_ws_request_has_subprotocol() {
        let req = ws_request("wss://h.example/ndt/v7/download?access_token=t").unwrap();
        assert_eq!(
            req.headers().get(SEC_WEBSOCKET_PROTOCOL).unwrap(),
            NDT7_SUBPROTOCOL
        );
        assert_eq!(req.uri().path(), "/ndt/v7/download");
        assert_eq!(req.uri().query(), Some("access_token=t"));
    }

    #[test]
    fn test_parse_locate_rejects_garbage() {
        assert!(parse_locate_response("not json").is_none());
        assert!(parse_locate_response("[]").is_none());
        assert!(parse_locate_response(r#"[{"urls":{}}]"#).is_none());
        assert!(parse_locate_response("{}").is_none());
        assert!(parse_locate_response(r#"{"results":[]}"#).is_none());
        // urls 缺失的服务应被跳过,而不是让整个解析失败(取下一台)
        let skipped = parse_locate_candidates(
            r#"{"results":[{"machine":"a"},{"urls":{"wss:///ndt/v7/download":"wss://h.example/d?t=1","wss:///ndt/v7/upload":"wss://h.example/u?t=1"}}]}"#,
        );
        assert_eq!(skipped.len(), 1, "缺 urls 的服务被跳过,下一台仍可解析");
        assert_eq!(skipped[0].base_url, "https://h.example");
    }

    /// 多候选:按响应顺序全部列出
    #[test]
    fn test_parse_locate_candidates_orders_all() {
        let fixture = r#"{"results":[
          {"urls":{"wss:///ndt/v7/download":"wss://a.example/d?t=1","wss:///ndt/v7/upload":"wss://a.example/u?t=1"}},
          {"urls":{"wss:///ndt/v7/download":"wss://b.example/d?t=1","wss:///ndt/v7/upload":"wss://b.example/u?t=1"}}
        ]}"#;
        let c = parse_locate_candidates(fixture);
        assert_eq!(c.len(), 2);
        assert_eq!(c[0].base_url, "https://a.example");
        assert_eq!(c[1].base_url, "https://b.example");
        assert_eq!(
            parse_locate_response(fixture).unwrap().base_url,
            "https://a.example"
        );
    }

    /// 真实网络:M-Lab Locate 发现 + NDT7 下载/上传(真实吞吐,非模拟)。
    /// 若发现或链路完全不可用,断言失败信息会如实说明。
    /// 需要真实网络,默认跳过;显式运行:`cargo test test_ndt7_real_network -- --ignored`
    #[test]
    #[ignore = "需要真实网络,离线/CI 下运行会失败"]
    fn test_ndt7_real_network() {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        rt.block_on(async {
            let adapter = Ndt7Adapter::new();
            let node = adapter
                .discover()
                .await
                .expect("M-Lab Locate 应发现至少一台可用 NDT7 服务");
            assert!(node.dl_url.starts_with("wss://") || node.dl_url.starts_with("ws://"));
            // 延迟:WS 握手往返
            let token = tokio_util::sync::CancellationToken::new();
            let rtt = adapter
                .ping_rtt(&node, &token)
                .await
                .expect("NDT7 WS 握手应成功");
            assert!(
                rtt > 0.0 && rtt < 20_000.0,
                "握手 RTT 应在合理范围,实际 {rtt} ms"
            );

            // 下载:3 并发流 3s,必须收到真实字节
            let dl = Arc::new(ThroughputCounters::default());
            adapter
                .run_download(
                    &node,
                    dl.clone(),
                    PhaseConfig {
                        duration: Duration::from_secs(3),
                        streams: 3,
                    },
                    token.child_token(),
                )
                .await;
            let dl_bytes = dl.total();
            assert!(
                dl_bytes > 100_000,
                "NDT7 下载应收到真实数据,实际 {dl_bytes} 字节"
            );
            let mbps = super::super::model::bytes_per_sec_to_mbps(dl_bytes as f64 / 3.0);
            eprintln!("NDT7 实测: 下载 {mbps:.1} Mbps / {dl_bytes} 字节 / RTT {rtt:.1} ms");

            // 上传:2 并发流 3s,必须发出真实字节
            let ul = Arc::new(ThroughputCounters::default());
            adapter
                .run_upload(
                    &node,
                    ul.clone(),
                    PhaseConfig {
                        duration: Duration::from_secs(3),
                        streams: 2,
                    },
                    token.child_token(),
                )
                .await;
            let ul_bytes = ul.total();
            assert!(
                ul_bytes > 50_000,
                "NDT7 上传应发出真实数据,实际 {ul_bytes} 字节"
            );
        });
    }
}

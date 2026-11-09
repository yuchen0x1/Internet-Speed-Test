//! 测速节点管理:列表获取(在线 + 内置兜底 + 自定义)、可用性探测、自动优选。
//!
//! 优选标准:综合考虑延迟、可用性、下载能力(2.5s 能力探测),
//! 不以地理距离推断网络质量。

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use futures_util::StreamExt;
use serde::Deserialize;
use tokio_util::sync::CancellationToken;

use super::adapter::{PhaseConfig, ProviderAdapter, ThroughputCounters};
use super::cloudflare::{builtin_node, CloudflareAdapter};
use super::librespeed::LibreSpeedAdapter;
use super::model::ServerNodeInfo;
use super::ndt7::Ndt7Adapter;
use super::util;

const LIST_URL: &str = "https://librespeed.org/backend-servers/servers.php";
const FALLBACK_LIST: &str = include_str!("servers_fallback.json");

/// LibreSpeed 官方服务器列表条目格式
#[derive(Debug, Deserialize)]
struct LsListEntry {
    name: String,
    server: String,
    #[serde(alias = "dlURL")]
    dl_url: String,
    #[serde(alias = "ulURL")]
    ul_url: String,
    #[serde(alias = "pingURL")]
    ping_url: String,
    #[serde(alias = "getIpURL")]
    getip_url: String,
}

pub struct ServerRegistry {
    libre: LibreSpeedAdapter,
    cloudflare: CloudflareAdapter,
    ndt7: Ndt7Adapter,
}

/// 节点探测结果
#[derive(Debug, Clone)]
pub struct ProbeOutcome {
    pub node: ServerNodeInfo,
    pub alive: bool,
    pub latency_ms: Option<f64>,
}

impl Default for ServerRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl ServerRegistry {
    pub fn new() -> Self {
        Self {
            libre: LibreSpeedAdapter::new(),
            cloudflare: CloudflareAdapter::new(),
            ndt7: Ndt7Adapter::new(),
        }
    }

    pub fn adapter_dyn(&self, node: &ServerNodeInfo) -> &dyn ProviderAdapter {
        self.adapter_for(node)
    }

    fn adapter_for(&self, node: &ServerNodeInfo) -> &dyn ProviderAdapter {
        if node.provider == super::model::PROVIDER_NDT7 {
            &self.ndt7
        } else if node.provider == "cloudflare" {
            &self.cloudflare
        } else {
            &self.libre
        }
    }

    /// 获取完整节点列表:在线列表优先;内置兜底始终并入并按 base_url 去重,
    /// 保证在线列表缺失/离线时国内等实测节点仍可选;始终附加 Cloudflare 与自定义节点。
    pub async fn list_servers(
        &self,
        custom_nodes: &[ServerNodeInfo],
        favorites: &HashMap<String, bool>,
    ) -> Vec<ServerNodeInfo> {
        // 先统一归一化尾斜杠,再去重 —— 否则"同一节点带/不带斜杠"会重复入列
        let mut nodes: Vec<ServerNodeInfo> = self.fetch_online_list().await.unwrap_or_default();
        nodes.extend(self.bundled_list());
        for n in &mut nodes {
            n.base_url = n.base_url.trim_end_matches('/').to_string();
        }
        let mut seen = std::collections::HashSet::new();
        nodes.retain(|n| seen.insert(n.base_url.clone()));
        // M-Lab NDT7:经 Locate API v2 就近发现(失败则跳过,由其他协议兜底)
        if let Some(n) = self.ndt7.discover().await {
            nodes.push(n);
        }
        nodes.push(builtin_node());
        for n in custom_nodes {
            nodes.push(n.clone());
        }
        for n in &mut nodes {
            // 再归一化一次:自定义节点 URL 可能由用户带尾斜杠保存
            n.base_url = n.base_url.trim_end_matches('/').to_string();
            n.favorite = favorites.get(&n.base_url).copied().unwrap_or(false);
            n.id = util::node_id(&n.base_url, &n.name);
        }
        nodes
    }

    /// 在线获取官方列表(8s 超时,失败返回 None)
    async fn fetch_online_list(&self) -> Option<Vec<ServerNodeInfo>> {
        let client = reqwest::Client::builder()
            .user_agent(format!("NetVisionPro/{}", env!("CARGO_PKG_VERSION")))
            .connect_timeout(Duration::from_secs(5))
            .timeout(Duration::from_secs(10))
            .build()
            .ok()?;
        let text = client.get(LIST_URL).send().await.ok()?.text().await.ok()?;
        let entries: Vec<LsListEntry> = serde_json::from_str(&text).ok()?;
        Some(
            entries
                .into_iter()
                .filter_map(|e| {
                    let base = e.server.trim_end_matches('/').to_string();
                    if base.is_empty() {
                        return None;
                    }
                    // 仅接受 HTTPS 节点(安全要求);官方列表中的 HTTP 节点跳过
                    if !base.starts_with("https://") {
                        return None;
                    }
                    Some(ServerNodeInfo {
                        id: util::node_id(&base, &e.name),
                        name: e.name.clone(),
                        provider: "librespeed".into(),
                        base_url: base,
                        dl_url: e.dl_url,
                        ul_url: e.ul_url,
                        ping_url: e.ping_url,
                        getip_url: e.getip_url,
                        location: ServerNodeInfo::location_from_name(&e.name),
                        favorite: false,
                        alive: None,
                        latency_ms: None,
                        allow_http: false,
                    })
                })
                .collect(),
        )
    }

    /// 内置兜底列表(应用构建时的官方列表快照)
    fn bundled_list(&self) -> Vec<ServerNodeInfo> {
        serde_json::from_str::<Vec<LsListEntry>>(FALLBACK_LIST)
            .unwrap_or_default()
            .into_iter()
            .filter_map(|e| {
                let base = e.server.trim_end_matches('/').to_string();
                if !base.starts_with("https://") {
                    return None;
                }
                Some(ServerNodeInfo {
                    id: util::node_id(&base, &e.name),
                    name: e.name,
                    provider: "librespeed".into(),
                    base_url: base,
                    dl_url: e.dl_url,
                    ul_url: e.ul_url,
                    ping_url: e.ping_url,
                    getip_url: e.getip_url,
                    location: None,
                    favorite: false,
                    alive: None,
                    latency_ms: None,
                    allow_http: false,
                })
            })
            .collect()
    }

    /// 并发探测节点可用性与延迟
    pub async fn probe_nodes(
        &self,
        nodes: &[ServerNodeInfo],
        concurrency: usize,
        token: &CancellationToken,
    ) -> Vec<ProbeOutcome> {
        let stream = futures_util::stream::iter(nodes.iter().cloned())
            .map(|node| async move {
                let adapter = self.adapter_for(&node);
                let mut best: Option<f64> = None;
                for _ in 0..2 {
                    if token.is_cancelled() {
                        break;
                    }
                    if let Ok(rtt) = adapter.ping_rtt(&node, token).await {
                        best = Some(best.map_or(rtt, |b: f64| b.min(rtt)));
                    }
                }
                ProbeOutcome {
                    node,
                    alive: best.is_some(),
                    latency_ms: best,
                }
            })
            .buffer_unordered(concurrency.max(1));
        stream.collect::<Vec<_>>().await
    }

    /// 自动优选节点:延迟为主,辅以 2.5s 单流下载能力探测(前 4 名)
    /// 返回 (选中节点, 探测结果列表)
    pub async fn auto_select(
        &self,
        nodes: &[ServerNodeInfo],
        token: &CancellationToken,
    ) -> (Option<ServerNodeInfo>, Vec<ProbeOutcome>) {
        let probes = self.probe_nodes(nodes, 8, token).await;
        let mut alive: Vec<ProbeOutcome> = probes.iter().filter(|p| p.alive).cloned().collect();
        if alive.is_empty() {
            return (None, probes);
        }
        alive.sort_by(|a, b| {
            a.latency_ms
                .partial_cmp(&b.latency_ms)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        // 对延迟最好的 4 个节点做能力探测(真实单流下载 2.5s)
        let candidates: Vec<ServerNodeInfo> =
            alive.iter().take(4).map(|p| p.node.clone()).collect();
        let cap_stream = futures_util::stream::iter(candidates)
            .map(|node| async move {
                let adapter = self.adapter_for(&node);
                let counters = Arc::new(ThroughputCounters::default());
                let cfg = PhaseConfig {
                    duration: Duration::from_millis(2500),
                    streams: 1,
                };
                adapter
                    .run_download(&node, counters.clone(), cfg, token.child_token())
                    .await;
                let mbps = super::model::bytes_per_sec_to_mbps(
                    counters.total() as f64 / 2.5,
                );
                (node, mbps)
            })
            .buffer_unordered(4);
        let caps: Vec<(ServerNodeInfo, f64)> = cap_stream.collect::<Vec<_>>().await;
        let max_latency = alive
            .iter()
            .filter_map(|p| p.latency_ms)
            .fold(1.0f64, f64::max);
        let max_mbps = caps.iter().map(|(_, m)| *m).fold(0.000001f64, f64::max);
        // 综合评分:延迟 65% + 下载能力 35%(归一化)
        let mut best: Option<(ServerNodeInfo, f64)> = None;
        for (node, mbps) in &caps {
            let Some(p) = alive.iter().find(|p| p.node.id == node.id) else {
                continue;
            };
            let lat = p.latency_ms.unwrap_or(999.0);
            let lat_score = 1.0 - (lat / max_latency).clamp(0.0, 1.0);
            let cap_score = (mbps / max_mbps).clamp(0.0, 1.0);
            let score = lat_score * 0.65 + cap_score * 0.35;
            if best.as_ref().map_or(true, |(_, s)| score > *s) {
                best = Some((node.clone(), score));
            }
        }
        let chosen = best.map(|(n, _)| n);
        (chosen, probes)
    }

    /// 使用指定节点执行单次延迟测量
    pub async fn ping_node(
        &self,
        node: &ServerNodeInfo,
        token: &CancellationToken,
    ) -> Result<f64, String> {
        self.adapter_for(node).ping_rtt(node, token).await
    }

    pub async fn node_ip_info(
        &self,
        node: &ServerNodeInfo,
        token: &CancellationToken,
    ) -> Option<(String, Option<String>)> {
        self.adapter_for(node).fetch_ip_info(node, token).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_bundled_list_normalizes_trailing_slash() {
        let reg = ServerRegistry::new();
        let nodes = reg.bundled_list();
        assert!(!nodes.is_empty(), "内置兜底列表不应为空");
        for n in &nodes {
            assert!(n.base_url.starts_with("https://"), "仅接受 HTTPS 节点: {n:?}");
            assert!(!n.base_url.ends_with('/'), "兜底节点不带尾斜杠: {n:?}");
        }
        // 国内节点存在且端点字段完整
        let cn = nodes.iter().find(|n| n.name.contains("China")).expect("应包含国内节点");
        assert_eq!(cn.base_url, "https://cs.lt6.ltd");
        assert_eq!(cn.dl_url, "backend/garbage.php");
        assert_eq!(cn.ul_url, "backend/empty.php");
        assert_eq!(cn.ping_url, "backend/empty.php");
        assert_eq!(cn.getip_url, "backend/getIP.php");
        // 拼接后仍是合法 HTTPS 绝对 URL
        assert!(cn.download_url().starts_with("https://cs.lt6.ltd/backend/"));
        assert!(cn.ping_endpoint().starts_with("https://cs.lt6.ltd/backend/"));
    }
}

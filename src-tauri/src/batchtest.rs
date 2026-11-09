//! 批量拨测(ITDOG 式):对多个目标并发执行真实 ICMP,逐样本事件上报。
//!
//! - 每个目标独立任务,样本间隔 250ms;IcmpSendEcho 免管理员
//! - IPv6 目标由 icmp_ping 自动回退系统 ping
//! - 域名解析分布:系统解析 + 4 家公共 DNS 交叉统计

use std::sync::{Arc, Mutex};

use serde::Serialize;
use tokio_util::sync::CancellationToken;

use crate::diagnostics;

struct BatchInner {
    /// 单调递增代次:区分"本次任务自然结束的清理"与"已被新任务接管"
    gen: u64,
    token: Option<CancellationToken>,
}

impl Default for BatchInner {
    fn default() -> Self {
        Self { gen: 0, token: None }
    }
}

#[derive(Default)]
pub struct BatchTestState {
    inner: Arc<Mutex<BatchInner>>,
}

#[derive(Debug, Clone, Serialize)]
pub struct BatchTick {
    pub target: String,
    pub rtt_ms: Option<f64>,
}

#[derive(Debug, Clone, Serialize)]
pub struct BatchTargetResult {
    pub target: String,
    pub sent: u32,
    pub recv: u32,
    pub loss_percent: f64,
    pub min_ms: Option<f64>,
    pub max_ms: Option<f64>,
    pub avg_ms: Option<f64>,
    pub latest_ms: Option<f64>,
    pub samples: Vec<Option<f64>>,
}

#[derive(Debug, Clone, Serialize)]
pub struct DnsDistEntry {
    pub server: String,
    /// 系统 | 公共
    pub source: String,
    pub ip: String,
    pub rtt_ms: Option<f64>,
}

#[derive(Debug, Clone, Serialize)]
pub struct DnsDistributionReport {
    pub domain: String,
    pub entries: Vec<DnsDistEntry>,
    /// IP -> 被多少家解析器返回
    pub distribution: Vec<(String, u32)>,
    pub resolvers: u32,
}

impl BatchTestState {
    pub fn start(
        &self,
        app: tauri::AppHandle,
        targets: Vec<String>,
        count: u32,
    ) -> Result<usize, String> {
        use tauri::Emitter as _;
        let count = count.clamp(1, 100);
        let valid: Vec<String> = targets
            .into_iter()
            .map(|t| t.trim().to_string())
            .filter(|t| !t.is_empty() && diagnostics::valid_host(t))
            .collect();
        if valid.is_empty() {
            // 先校验输入再停旧任务:非法输入不应中断正在运行的拨测
            return Err("没有有效的目标主机".into());
        }
        let (token, my_gen) = {
            let mut inner = self.inner.lock().unwrap();
            if let Some(t) = inner.token.take() {
                t.cancel();
            }
            inner.gen += 1;
            let token = CancellationToken::new();
            inner.token = Some(token.clone());
            (token, inner.gen)
        };
        let n = valid.len();
        let slot = self.inner.clone();
        tauri::async_runtime::spawn(async move {
            let mut set = tokio::task::JoinSet::new();
            for target in valid {
                let app = app.clone();
                let token = token.clone();
                set.spawn(async move {
                    let mut samples: Vec<Option<f64>> = Vec::with_capacity(count as usize);
                    for i in 0..count {
                        if token.is_cancelled() {
                            break;
                        }
                        let p = diagnostics::icmp_ping(&target, 1, 1200).await;
                        let rtt = p.rtts_ms.first().copied();
                        samples.push(rtt);
                        let _ = app.emit(
                            "batchtest://tick",
                            &BatchTick { target: target.clone(), rtt_ms: rtt },
                        );
                        if i + 1 < count {
                            tokio::time::sleep(std::time::Duration::from_millis(250)).await;
                        }
                    }
                    let recv = samples.iter().filter(|s| s.is_some()).count();
                    let ok: Vec<f64> = samples.iter().filter_map(|s| *s).collect();
                    BatchTargetResult {
                        target,
                        sent: samples.len() as u32,
                        recv: recv as u32,
                        loss_percent: if samples.is_empty() {
                            0.0
                        } else {
                            (samples.len() - recv) as f64 * 100.0 / samples.len() as f64
                        },
                        min_ms: ok.iter().cloned().fold(None, |a: Option<f64>, v| {
                            Some(a.map_or(v, |m: f64| m.min(v)))
                        }),
                        max_ms: ok.iter().cloned().fold(None, |a: Option<f64>, v| {
                            Some(a.map_or(v, |m: f64| m.max(v)))
                        }),
                        avg_ms: if ok.is_empty() {
                            None
                        } else {
                            Some(ok.iter().sum::<f64>() / ok.len() as f64)
                        },
                        latest_ms: *samples.last().unwrap_or(&None),
                        samples,
                    }
                });
            }
            let mut results = Vec::new();
            while let Some(res) = set.join_next().await {
                if let Ok(r) = res {
                    let _ = app.emit("batchtest://target_done", &r);
                    results.push(r);
                }
            }
            // 自然结束后复位运行状态;若已被新任务接管(代次变化)则不动新任务的 token
            {
                let mut inner = slot.lock().unwrap();
                if inner.gen == my_gen {
                    inner.token = None;
                }
            }
            let _ = app.emit("batchtest://all_done", &results.len());
        });
        Ok(n)
    }

    pub fn stop(&self) {
        let token = {
            let mut inner = self.inner.lock().unwrap();
            inner.token.take()
        };
        if let Some(t) = token {
            t.cancel();
        }
    }

    pub fn is_running(&self) -> bool {
        self.inner.lock().unwrap().token.is_some()
    }
}

/// 域名解析分布:系统解析 + 公共 DNS 交叉,统计每个 IP 被多少家返回
pub async fn dns_distribution(domain: &str) -> Result<DnsDistributionReport, String> {
    let domain = domain.trim().to_string();
    if !diagnostics::valid_host(&domain) {
        return Err("无效的域名".into());
    }
    let mut entries: Vec<DnsDistEntry> = Vec::new();
    let mut resolvers = 0u32;

    // 系统解析
    if let Ok(addrs) = tokio::net::lookup_host((domain.as_str(), 0u16)).await {
        let ips: Vec<String> = addrs.map(|a| a.ip().to_string()).collect();
        if !ips.is_empty() {
            resolvers += 1;
            for ip in ips {
                entries.push(DnsDistEntry {
                    server: "系统".into(),
                    source: "系统".into(),
                    ip,
                    rtt_ms: None,
                });
            }
        }
    }
    // 公共 DNS
    for s in ["223.5.5.5", "119.29.29.29", "8.8.8.8", "1.1.1.1"] {
        if let Ok(ip) = s.parse::<std::net::IpAddr>() {
            let p = diagnostics::dns_udp_probe(ip, &domain).await;
            if p.ok {
                if let Some(a) = p.resolved_ip {
                    resolvers += 1;
                    entries.push(DnsDistEntry {
                        server: s.to_string(),
                        source: "公共".into(),
                        ip: a,
                        rtt_ms: p.rtt_ms,
                    });
                }
            }
        }
    }
    // 分布统计:IP -> 返回它的解析器数(按服务器去重)
    let mut dist: std::collections::BTreeMap<String, u32> = std::collections::BTreeMap::new();
    let mut seen_per_server: std::collections::HashSet<String> = std::collections::HashSet::new();
    for e in &entries {
        let key = format!("{}|{}", e.server, e.ip);
        if seen_per_server.insert(key) {
            *dist.entry(e.ip.clone()).or_insert(0) += 1;
        }
    }
    Ok(DnsDistributionReport {
        domain,
        entries,
        distribution: dist.into_iter().collect(),
        resolvers,
    })
}

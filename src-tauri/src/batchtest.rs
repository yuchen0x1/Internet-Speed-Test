//! 批量拨测(ITDOG 式):对多个目标并发执行真实 ICMP,逐样本事件上报。
//!
//! - 最多 32 个去重目标、8 路并发，样本间隔 250ms;IcmpSendEcho 免管理员
//! - IPv6 目标由 icmp_ping 自动回退系统 ping
//! - 域名解析分布:系统解析 + 4 家公共 DNS 交叉统计

use std::sync::{Arc, Mutex};

use serde::Serialize;
use tokio_util::sync::CancellationToken;

use crate::diagnostics;

pub const MAX_TARGETS: usize = 32;
pub const MAX_CONCURRENCY: usize = 8;

#[derive(Default)]
struct BatchInner {
    gen: u64,
    token: Option<CancellationToken>,
    snapshot: Option<BatchSnapshot>,
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

/// Snapshot IPC contains all results; update events carry only the changed target.
/// The revision orders lifecycle changes, while sent orders each target's samples.
#[derive(Debug, Clone, Serialize)]
pub struct BatchSnapshot {
    pub run_id: u64,
    pub started_at: String,
    pub revision: u64,
    pub running: bool,
    pub cancelled: bool,
    pub count: u32,
    pub targets: Vec<String>,
    pub results: Vec<BatchTargetResult>,
}

pub fn normalize_targets(targets: Vec<String>) -> Result<Vec<String>, String> {
    let mut valid = Vec::new();
    let mut seen = std::collections::HashSet::new();
    // Bound even duplicate/blank-heavy IPC payloads before processing them.
    if targets.len() > 256 {
        return Err("输入行数过多，最多 256 行、32 个不同目标".into());
    }
    for target in targets {
        let target = target.trim().to_ascii_lowercase();
        if target.is_empty() {
            continue;
        }
        let is_ip = target.parse::<std::net::IpAddr>().is_ok();
        let is_domain = target.len() <= 253
            && target.trim_end_matches('.').split('.').all(|label| {
                !label.is_empty()
                    && label.len() <= 63
                    && !label.starts_with('-')
                    && !label.ends_with('-')
                    && label
                        .bytes()
                        .all(|c| c.is_ascii_alphanumeric() || c == b'-')
            });
        if !is_ip && !is_domain {
            return Err("存在无效目标，请输入主机名或 IP，不要包含协议、路径或端口".into());
        }
        // Canonicalize equivalent IP spellings and DNS trailing dots as well.
        let target = target
            .parse::<std::net::IpAddr>()
            .map(|ip| ip.to_string())
            .unwrap_or_else(|_| target.trim_end_matches('.').to_string());
        if seen.insert(target.clone()) {
            valid.push(target);
        }
        if valid.len() > MAX_TARGETS {
            return Err("最多支持 32 个不同目标".into());
        }
    }
    if valid.is_empty() {
        return Err("请至少输入一个有效目标".into());
    }
    Ok(valid)
}

fn summarize(target: String, samples: Vec<Option<f64>>) -> BatchTargetResult {
    let ok: Vec<f64> = samples.iter().filter_map(|v| *v).collect();
    let sent = samples.len() as u32;
    let recv = ok.len() as u32;
    BatchTargetResult {
        target,
        sent,
        recv,
        loss_percent: if sent == 0 {
            0.0
        } else {
            (sent - recv) as f64 * 100.0 / sent as f64
        },
        min_ms: ok.iter().copied().reduce(f64::min),
        max_ms: ok.iter().copied().reduce(f64::max),
        avg_ms: if ok.is_empty() {
            None
        } else {
            Some(ok.iter().sum::<f64>() / ok.len() as f64)
        },
        latest_ms: samples.last().copied().flatten(),
        samples,
    }
}

impl BatchInner {
    fn begin(
        &mut self,
        targets: Vec<String>,
        count: u32,
    ) -> Result<(CancellationToken, u64), String> {
        if self.token.is_some() {
            return Err("上一轮拨测尚未结束，请等待或停止后重试".into());
        }
        self.gen += 1;
        let token = CancellationToken::new();
        self.token = Some(token.clone());
        self.snapshot = Some(BatchSnapshot {
            run_id: self.gen,
            started_at: chrono::Local::now().to_rfc3339(),
            revision: 0,
            running: true,
            cancelled: false,
            count: count.clamp(1, 100),
            targets,
            results: Vec::new(),
        });
        Ok((token, self.gen))
    }

    fn sample(&mut self, run_id: u64, result: BatchTargetResult) -> Option<BatchSnapshot> {
        let s = self.snapshot.as_mut()?;
        if s.run_id != run_id || !s.running || s.cancelled {
            return None;
        }
        s.results.retain(|r| r.target != result.target);
        s.results.push(result.clone());
        s.revision += 1;
        Some(BatchSnapshot {
            results: vec![result],
            ..s.clone()
        })
    }

    fn finish(&mut self, run_id: u64) -> Option<BatchSnapshot> {
        let s = self.snapshot.as_mut()?;
        if s.run_id != run_id || !s.running {
            return None;
        }
        s.running = false;
        s.revision += 1;
        self.token = None;
        Some(s.clone())
    }
}

/// Bound native probes, DNS timeouts, and cancellation, including queued targets.
async fn run_probes<F, Fut, S>(
    targets: Vec<String>,
    count: u32,
    token: CancellationToken,
    probe: F,
    sample: S,
) where
    F: Fn(String) -> Fut,
    Fut: std::future::Future<Output = Option<f64>>,
    S: Fn(String, Vec<Option<f64>>),
{
    use futures_util::StreamExt;
    futures_util::stream::iter(targets).for_each_concurrent(MAX_CONCURRENCY, |target| {
        let token = &token;
        let probe = &probe;
        let sample = &sample;
        async move {
            let mut samples = Vec::new();
            for i in 0..count {
                let rtt = tokio::select! {
                    biased;
                    _ = token.cancelled() => return,
                    rtt = tokio::time::timeout(std::time::Duration::from_secs(5), probe(target.clone())) => rtt.unwrap_or(None),
                };
                if token.is_cancelled() { return; }
                samples.push(rtt);
                sample(target.clone(), samples.clone());
                if i + 1 < count {
                    tokio::select! {
                        _ = token.cancelled() => return,
                        _ = tokio::time::sleep(std::time::Duration::from_millis(250)) => {}
                    }
                }
            }
        }
    }).await;
}

impl BatchTestState {
    pub fn start(
        &self,
        app: tauri::AppHandle,
        targets: Vec<String>,
        count: u32,
    ) -> Result<usize, String> {
        use tauri::Emitter as _;
        let targets = normalize_targets(targets)?;
        let count = count.clamp(1, 100);
        let n = targets.len();
        let (token, run_id) = crate::safe_lock::lock(&self.inner).begin(targets.clone(), count)?;
        let slot = self.inner.clone();
        tauri::async_runtime::spawn(async move {
            run_probes(
                targets,
                count,
                token,
                |target| async move {
                    diagnostics::icmp_ping(&target, 1, 1200)
                        .await
                        .rtts_ms
                        .first()
                        .copied()
                },
                |target, samples| {
                    let result = summarize(target.clone(), samples);
                    // Publish under the state lock: finish/start cannot overtake a sample.
                    let mut inner = crate::safe_lock::lock(&slot);
                    if let Some(update) = inner.sample(run_id, result.clone()) {
                        let _ = app.emit("batchtest://update", &update);
                        // Preserve the legacy event and command contracts.
                        let _ = app.emit(
                            "batchtest://tick",
                            &BatchTick {
                                target,
                                rtt_ms: result.latest_ms,
                            },
                        );
                        if result.sent == count {
                            let _ = app.emit("batchtest://target_done", &result);
                        }
                    }
                },
            )
            .await;
            let mut inner = crate::safe_lock::lock(&slot);
            if let Some(snapshot) = inner.finish(run_id) {
                let _ = app.emit("batchtest://update", &snapshot);
                let _ = app.emit("batchtest://all_done", &snapshot.results.len());
            }
        });
        Ok(n)
    }

    pub fn stop(&self) {
        let mut inner = crate::safe_lock::lock(&self.inner);
        if let Some(t) = &inner.token {
            t.cancel();
        }
        if let Some(s) = &mut inner.snapshot {
            if s.running {
                s.cancelled = true;
                s.revision += 1;
            }
        }
    }

    pub fn is_running(&self) -> bool {
        crate::safe_lock::lock(&self.inner).token.is_some()
    }

    pub fn snapshot(&self) -> Option<BatchSnapshot> {
        crate::safe_lock::lock(&self.inner).snapshot.clone()
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn target_validation_is_atomic_deduplicated_and_bounded() {
        let input = [
            " Example.COM. ",
            "example.com",
            "::1",
            "0:0:0:0:0:0:0:1",
            "127.0.0.1",
        ];
        assert_eq!(
            normalize_targets(input.map(str::to_string).to_vec()).unwrap(),
            ["example.com", "::1", "127.0.0.1"]
        );
        for bad in [
            "https://example.com",
            "host:443",
            "-bad",
            "a..b",
            "bad/host",
            "",
            " ",
        ] {
            assert!(normalize_targets(vec![bad.into()]).is_err(), "{bad}");
        }
        assert!(normalize_targets((0..33).map(|i| format!("host{i}.test")).collect()).is_err());
        assert!(normalize_targets(vec!["x".into(); 257]).is_err());
    }

    #[test]
    fn lifecycle_rejects_overlap_and_stale_events() {
        let mut inner = BatchInner::default();
        let (_, first) = inner.begin(vec!["localhost".into()], 10).unwrap();
        assert!(inner.begin(vec!["localhost".into()], 10).is_err());
        inner.sample(first, summarize("localhost".into(), vec![Some(5.0), None]));
        inner.snapshot.as_mut().unwrap().cancelled = true;
        assert!(inner
            .sample(first, summarize("localhost".into(), vec![Some(99.0)]))
            .is_none());
        let final_state = inner.finish(first).unwrap();
        assert!(final_state.cancelled);
        assert_eq!(final_state.results[0].sent, 2);
        assert_eq!(final_state.results[0].loss_percent, 50.0);
        let (_, next) = inner.begin(vec!["localhost".into()], 10).unwrap();
        assert!(inner.finish(first).is_none());
        assert!(inner
            .sample(first, summarize("localhost".into(), vec![None]))
            .is_none());
        assert_eq!(inner.snapshot.unwrap().run_id, next);
    }

    #[tokio::test]
    async fn probes_limit_concurrency_and_preserve_timeouts() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        let active = AtomicUsize::new(0);
        let peak = AtomicUsize::new(0);
        let received = Mutex::new(Vec::new());
        run_probes(
            (0..32).map(|i| i.to_string()).collect(),
            1,
            CancellationToken::new(),
            |_| async {
                let n = active.fetch_add(1, Ordering::SeqCst) + 1;
                peak.fetch_max(n, Ordering::SeqCst);
                tokio::time::sleep(std::time::Duration::from_millis(10)).await;
                active.fetch_sub(1, Ordering::SeqCst);
                None
            },
            |_, samples| received.lock().unwrap().push(samples),
        )
        .await;
        assert_eq!(peak.load(Ordering::SeqCst), MAX_CONCURRENCY);
        assert_eq!(received.lock().unwrap().len(), 32);
        assert!(received.lock().unwrap().iter().all(|v| v == &[None]));
    }

    #[tokio::test]
    async fn cancellation_interrupts_inflight_and_queued_probes() {
        let token = CancellationToken::new();
        let cancel = token.clone();
        let stop = async move {
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
            cancel.cancel();
        };
        let run = run_probes(
            vec!["localhost".into(); 32],
            100,
            token,
            |_| std::future::pending::<Option<f64>>(),
            |_, _| panic!("cancelled probe emitted a sample"),
        );
        tokio::time::timeout(std::time::Duration::from_millis(300), async {
            tokio::join!(run, stop);
        })
        .await
        .unwrap();
    }

    #[tokio::test]
    #[ignore = "real Windows loopback ICMP"]
    async fn loopback_uses_real_icmp_pipeline() {
        let samples = Mutex::new(Vec::new());
        run_probes(
            vec!["127.0.0.1".into()],
            2,
            CancellationToken::new(),
            |host| async move {
                diagnostics::icmp_ping(&host, 1, 1200)
                    .await
                    .rtts_ms
                    .first()
                    .copied()
            },
            |_, values| *samples.lock().unwrap() = values,
        )
        .await;
        let values = samples.lock().unwrap();
        assert_eq!(values.len(), 2);
        assert!(values.iter().all(Option::is_some));
    }
}

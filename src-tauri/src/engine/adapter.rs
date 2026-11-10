//! 测速服务适配器层:统一不同提供商(LibreSpeed / Cloudflare)的访问协议。

use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use tokio_util::sync::CancellationToken;

use super::model::ServerNodeInfo;

/// 吞吐计数器(多流共享,原子累加)
#[derive(Debug, Default)]
pub struct ThroughputCounters {
    pub total_bytes: AtomicU64,
    /// 吞吐阶段 HTTP 请求失败次数(与 ICMP 丢包严格区分)
    pub failures: AtomicU32,
}

impl ThroughputCounters {
    pub fn add_bytes(&self, n: u64) {
        self.total_bytes.fetch_add(n, Ordering::Relaxed);
    }
    pub fn total(&self) -> u64 {
        self.total_bytes.load(Ordering::Relaxed)
    }
    pub fn add_failure(&self) {
        self.failures.fetch_add(1, Ordering::Relaxed);
    }
    pub fn failure_count(&self) -> u32 {
        self.failures.load(Ordering::Relaxed)
    }
}

/// 单阶段(下载或上传)运行参数
#[derive(Debug, Clone, Copy)]
pub struct PhaseConfig {
    pub duration: Duration,
    pub streams: usize,
}

/// 提供商适配器:屏蔽底层协议差异
#[async_trait]
pub trait ProviderAdapter: Send + Sync {
    /// 协议标识(阶段日志与事件标注用)
    fn provider(&self) -> &'static str;

    /// 单次 HTTP 往返延迟(ms)。返回响应头到达耗时 ≈ RTT + 服务器处理。
    async fn ping_rtt(
        &self,
        node: &ServerNodeInfo,
        token: &CancellationToken,
    ) -> Result<f64, String>;

    /// 执行下载阶段:持续 duration,向 counters 累计字节数。流失败计入 failures。
    async fn run_download(
        &self,
        node: &ServerNodeInfo,
        counters: Arc<ThroughputCounters>,
        cfg: PhaseConfig,
        token: CancellationToken,
    );

    /// 执行上传阶段(自适应分块)
    async fn run_upload(
        &self,
        node: &ServerNodeInfo,
        counters: Arc<ThroughputCounters>,
        cfg: PhaseConfig,
        token: CancellationToken,
    );

    /// 获取公网 IP / ISP(尽力而为,失败返回 None)
    async fn fetch_ip_info(
        &self,
        node: &ServerNodeInfo,
        token: &CancellationToken,
    ) -> Option<(String, Option<String>)>;
}

/// 带取消的 HTTP 延迟测量(所有提供商通用)
pub async fn measure_http_rtt(
    client: &reqwest::Client,
    url: &str,
    token: &CancellationToken,
) -> Result<f64, String> {
    let nonce = chrono::Utc::now().timestamp_nanos_opt().unwrap_or_default();
    let t0 = std::time::Instant::now();
    let fut = client
        .get(url)
        .header("Cache-Control", "no-cache")
        .header("Pragma", "no-cache")
        .query(&[("_t", nonce.to_string())])
        .timeout(Duration::from_secs(4))
        .send();
    let resp = match token.run_until_cancelled(fut).await {
        Some(r) => r.map_err(|e| format!("请求失败: {e}"))?,
        None => return Err("已取消".into()),
    };
    if !resp.status().is_success() {
        return Err(format!("HTTP {}", resp.status().as_u16()));
    }
    let rtt = t0.elapsed().as_secs_f64() * 1000.0;
    // 读取小响应体(空.php / bytes=0),确保连接完整
    let _ = token.run_until_cancelled(resp.bytes()).await;
    Ok(rtt)
}

/// 生成缓存穿透参数
pub fn cache_buster() -> String {
    let n = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.subsec_nanos())
        .unwrap_or(0);
    format!("nv{n}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_counters() {
        let c = ThroughputCounters::default();
        c.add_bytes(1000);
        c.add_bytes(500);
        assert_eq!(c.total(), 1500);
        c.add_failure();
        assert_eq!(c.failure_count(), 1);
    }
}

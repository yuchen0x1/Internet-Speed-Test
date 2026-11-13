//! 引擎共享数据模型:节点、事件、结果。
//! 所有跨进程(Frontend)传输的结构均在此定义,保证前后端字段一致。

use serde::ser::SerializeStruct;
use serde::{Deserialize, Serialize};

/// 测速服务提供商类型
pub const PROVIDER_LIBRESPEED: &str = "librespeed";
pub const PROVIDER_CLOUDFLARE: &str = "cloudflare";
pub const PROVIDER_NDT7: &str = "ndt7";
pub const PROVIDER_CUSTOM: &str = "custom";

/// 测速节点(统一抽象,兼容 LibreSpeed 协议与 Cloudflare)
#[derive(Debug, Clone, Deserialize, PartialEq)]
#[serde(default)]
pub struct ServerNodeInfo {
    /// 唯一 ID(由 URL+名称 哈希生成)
    pub id: String,
    /// 显示名称,如 "Frankfurt, Germany (Clouvider)"
    pub name: String,
    /// 提供商: librespeed | cloudflare | custom
    pub provider: String,
    /// 基础 URL,如 https://host/backend
    pub base_url: String,
    /// 下载端点(相对 base_url,或完整 URL)
    pub dl_url: String,
    /// 上传端点
    pub ul_url: String,
    /// 延迟测量端点
    pub ping_url: String,
    /// 客户端 IP 信息端点
    pub getip_url: String,
    /// 地理位置(从名称解析)
    pub location: Option<String>,
    /// 是否收藏
    pub favorite: bool,
    /// 最近探测结果
    pub alive: Option<bool>,
    pub latency_ms: Option<f64>,
    /// 自定义节点是否允许 HTTP(默认强制 HTTPS)
    pub allow_http: bool,
}

impl Serialize for ServerNodeInfo {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let (dl_url, ul_url, ping_url) = if self.provider == PROVIDER_NDT7 {
            (String::new(), String::new(), String::new())
        } else {
            (
                self.dl_url.clone(),
                self.ul_url.clone(),
                self.ping_url.clone(),
            )
        };
        let mut state = serializer.serialize_struct("ServerNodeInfo", 13)?;
        state.serialize_field("id", &self.id)?;
        state.serialize_field("name", &self.name)?;
        state.serialize_field("provider", &self.provider)?;
        state.serialize_field("base_url", &self.base_url)?;
        state.serialize_field("dl_url", &dl_url)?;
        state.serialize_field("ul_url", &ul_url)?;
        state.serialize_field("ping_url", &ping_url)?;
        state.serialize_field("getip_url", &self.getip_url)?;
        state.serialize_field("location", &self.location)?;
        state.serialize_field("favorite", &self.favorite)?;
        state.serialize_field("alive", &self.alive)?;
        state.serialize_field("latency_ms", &self.latency_ms)?;
        state.serialize_field("allow_http", &self.allow_http)?;
        state.end()
    }
}

impl Default for ServerNodeInfo {
    fn default() -> Self {
        Self {
            id: String::new(),
            name: String::new(),
            provider: PROVIDER_LIBRESPEED.into(),
            base_url: String::new(),
            dl_url: "garbage.php".into(),
            ul_url: "empty.php".into(),
            ping_url: "empty.php".into(),
            getip_url: "getIP.php".into(),
            location: None,
            favorite: false,
            alive: None,
            latency_ms: None,
            allow_http: false,
        }
    }
}

impl ServerNodeInfo {
    /// 解析完整的下载 URL
    pub fn download_url(&self) -> String {
        join_url(&self.base_url, &self.dl_url)
    }
    pub fn upload_url(&self) -> String {
        join_url(&self.base_url, &self.ul_url)
    }
    pub fn ping_endpoint(&self) -> String {
        join_url(&self.base_url, &self.ping_url)
    }
    pub fn ip_endpoint(&self) -> String {
        join_url(&self.base_url, &self.getip_url)
    }
    /// 从 "City, Country (Sponsor)" 名称中解析地理位置
    pub fn location_from_name(name: &str) -> Option<String> {
        let no_sponsor = name.split('(').next().unwrap_or(name).trim();
        if no_sponsor.is_empty() {
            None
        } else {
            Some(no_sponsor.to_string())
        }
    }
}

/// 相对 URL 拼接:base 尾部与 rel 头部斜杠归一;http(s)/ws(s) 视为绝对 URL 原样返回
pub fn join_url(base: &str, rel: &str) -> String {
    let base = base.trim_end_matches('/');
    let absolute = ["http://", "https://", "ws://", "wss://"]
        .iter()
        .any(|p| rel.starts_with(p));
    if absolute {
        rel.to_string()
    } else {
        format!("{}/{}", base, rel.trim_start_matches('/'))
    }
}

/// 测试阶段
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum TestPhase {
    Idle,
    Preparing,
    Latency,
    Download,
    Upload,
    Quality,
    Finished,
}

impl TestPhase {
    pub fn as_str(&self) -> &'static str {
        match self {
            TestPhase::Idle => "idle",
            TestPhase::Preparing => "preparing",
            TestPhase::Latency => "latency",
            TestPhase::Download => "download",
            TestPhase::Upload => "upload",
            TestPhase::Quality => "quality",
            TestPhase::Finished => "finished",
        }
    }
}

/// 引擎实时事件(前端按 type 分发)
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum EngineEvent {
    /// 阶段切换
    Phase { phase: String, message: String },
    /// 选中节点
    Server { node: ServerNodeInfo },
    /// 延迟采样(rtt_ms 单次往返)
    Latency { rtt_ms: f64 },
    /// 实时速度采样
    Speed {
        direction: String, // download | upload
        mbps: f64,
        cumulative_mb: f64,
    },
    /// 负载下延迟
    LoadedLatency {
        direction: String, // download | upload
        rtt_ms: f64,
    },
    /// 进度(0-100)
    Progress { percent: f32 },
    /// 完成
    Finished { result: Box<TestResult> },
    /// 失败
    Failed { error: String },
    /// 已取消
    Cancelled,
}

/// 单次测速结果(前端展示 + 历史入库)
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct TestResult {
    pub id: String,
    pub started_at: String,
    pub finished_at: String,
    pub duration_ms: u64,
    /// completed | cancelled | failed
    pub status: String,
    pub error: Option<String>,
    /// 测量结果与保存结果分开报告，写入失败不改变真实测速状态。
    pub history_save_error: Option<String>,

    pub download_mbps: f64,
    pub download_peak_mbps: f64,
    pub upload_mbps: Option<f64>,
    pub upload_peak_mbps: Option<f64>,

    pub idle_ping_ms: f64,
    pub ping_min_ms: f64,
    pub ping_max_ms: f64,
    pub jitter_ms: f64,
    pub loaded_down_ms: Option<f64>,
    pub loaded_up_ms: Option<f64>,
    /// ICMP 实测丢包率(HTTP 失败不计入此项)
    pub packet_loss_percent: Option<f64>,
    /// 吞吐阶段 HTTP 请求失败次数(与丢包严格区分)
    pub http_failures: u32,

    pub server: Option<ServerNodeInfo>,
    pub public_ip: Option<String>,
    pub isp: Option<String>,
    pub network_type: String,
    pub interface_name: String,

    /// 实时样本 [时间偏移ms, 瞬时Mbps]
    pub down_samples: Vec<[f64; 2]>,
    pub up_samples: Vec<[f64; 2]>,
    /// 延迟样本 [时间偏移ms, RTT ms]
    pub latency_samples: Vec<[f64; 2]>,

    /// 质量评级与分析说明(由 ResultAnalyzer 依据真实数据生成)
    pub quality: Option<QualityAssessment>,
    /// 本次测试条件说明(时长/并发等)
    pub conditions: Vec<String>,
}

impl Default for TestResult {
    fn default() -> Self {
        Self {
            id: String::new(),
            started_at: String::new(),
            finished_at: String::new(),
            duration_ms: 0,
            status: "completed".into(),
            history_save_error: None,
            error: None,
            download_mbps: 0.0,
            download_peak_mbps: 0.0,
            upload_mbps: None,
            upload_peak_mbps: None,
            idle_ping_ms: 0.0,
            ping_min_ms: 0.0,
            ping_max_ms: 0.0,
            jitter_ms: 0.0,
            loaded_down_ms: None,
            loaded_up_ms: None,
            packet_loss_percent: None,
            http_failures: 0,
            server: None,
            public_ip: None,
            isp: None,
            network_type: "unknown".into(),
            interface_name: String::new(),
            down_samples: Vec::new(),
            up_samples: Vec::new(),
            latency_samples: Vec::new(),
            quality: None,
            conditions: Vec::new(),
        }
    }
}

/// 网络质量评级(可解释)
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct QualityAssessment {
    /// 0-100
    pub score: u32,
    /// 优秀 | 良好 | 一般 | 较差
    pub rating: String,
    /// 依据真实测试数据生成的分析说明
    pub notes: Vec<String>,
}

/// 引擎运行配置(由设置换算)
#[derive(Debug, Clone)]
pub struct EngineConfig {
    pub duration_secs: u64,
    pub down_streams: usize,
    pub up_streams: usize,
    /// 精度模式:更长时长、更多样本
    pub precision: bool,
    pub auto_select: bool,
    pub manual_node_url: Option<String>,
}

impl Default for EngineConfig {
    fn default() -> Self {
        Self {
            duration_secs: 10,
            down_streams: 6,
            up_streams: 4,
            precision: false,
            auto_select: true,
            manual_node_url: None,
        }
    }
}

/// 字节/秒 -> Mbps(统一换算入口,避免各处转换不一致)
#[inline]
pub fn bytes_per_sec_to_mbps(bps: f64) -> f64 {
    bps * 8.0 / 1_000_000.0
}

/// Mbps -> 字节/秒(与 bytes_per_sec_to_mbps 互逆)。当前仅用于换算往返测试。
#[cfg(test)]
#[inline]
pub fn mbps_to_bytes_per_sec(mbps: f64) -> f64 {
    mbps * 1_000_000.0 / 8.0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ndt7_access_tokens_are_not_serialized() {
        let node = ServerNodeInfo {
            provider: PROVIDER_NDT7.into(),
            dl_url: "wss://mlab.example/ndt/v7/download?access_token=secret".into(),
            ul_url: "wss://mlab.example/ndt/v7/upload?access_token=secret".into(),
            ping_url: "wss://mlab.example/ndt/v7/download?access_token=secret".into(),
            ..Default::default()
        };
        let json = serde_json::to_string(&node).unwrap();
        let public: ServerNodeInfo = serde_json::from_str(&json).unwrap();
        assert!(!json.contains("secret"));
        assert!(public.dl_url.is_empty());
        assert!(public.ul_url.is_empty());
        assert!(public.ping_url.is_empty());
    }

    #[test]
    fn test_unit_conversion() {
        // 1 MB/s = 8 Mbps
        assert!((bytes_per_sec_to_mbps(1_000_000.0) - 8.0).abs() < 1e-9);
        assert!((mbps_to_bytes_per_sec(8.0) - 1_000_000.0).abs() < 1e-9);
        // 125 Mbps 单流实测值往返
        let bps = 15_623_082.0;
        let round = mbps_to_bytes_per_sec(bytes_per_sec_to_mbps(bps));
        assert!((round - bps).abs() < 1e-6);
    }

    #[test]
    fn test_join_url() {
        assert_eq!(
            join_url("https://a.com/backend/", "garbage.php"),
            "https://a.com/backend/garbage.php"
        );
        assert_eq!(
            join_url("https://a.com", "backend/garbage.php"),
            "https://a.com/backend/garbage.php"
        );
        assert_eq!(
            join_url("https://a.com/x", "https://b.com/y"),
            "https://b.com/y"
        );
        // WebSocket 绝对 URL(NDT7)同样原样返回,不被拼接成 base/rel
        assert_eq!(
            join_url(
                "https://a.com",
                "wss://b.com/ndt/v7/download?access_token=t"
            ),
            "wss://b.com/ndt/v7/download?access_token=t"
        );
        assert_eq!(join_url("https://a.com", "ws://b.com/x"), "ws://b.com/x");
    }

    #[test]
    fn test_location_parse() {
        assert_eq!(
            ServerNodeInfo::location_from_name("Frankfurt, Germany (Clouvider)"),
            Some("Frankfurt, Germany".into())
        );
        assert_eq!(
            ServerNodeInfo::location_from_name("Las Vegas, USA (Sharktech)"),
            Some("Las Vegas, USA".into())
        );
    }
}

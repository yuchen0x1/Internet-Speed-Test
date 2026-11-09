//! 引擎工具:URL 安全校验、主机提取、随机数据池。

use reqwest::Url;

/// 校验测速节点 URL:仅允许 http/https;默认强制 HTTPS。
/// 返回归一化后的 URL 字符串。
pub fn validate_server_url(raw: &str, allow_http: bool) -> Result<String, String> {
    let url = Url::parse(raw.trim()).map_err(|e| format!("无效 URL: {e}"))?;
    match url.scheme() {
        "https" => Ok(url.to_string().trim_end_matches('/').to_string()),
        "http" if allow_http => Ok(url.to_string().trim_end_matches('/').to_string()),
        "http" => Err("出于安全考虑,测速节点必须使用 HTTPS(自定义节点可显式允许 HTTP)".into()),
        _ => Err(format!("不支持的协议: {}", url.scheme())),
    }
}

/// 从 URL 提取主机名(用于 ICMP ping / tracert)
pub fn extract_host(raw: &str) -> Option<String> {
    let url = Url::parse(raw).ok()?;
    url.host_str().map(|h| h.to_string())
}

/// 简单稳定哈希(FNV-1a 64),用于节点 ID 生成
pub fn fnv1a(data: &str) -> u64 {
    let mut hash: u64 = 0xcbf29ce484222325;
    for b in data.as_bytes() {
        hash ^= *b as u64;
        hash = hash.wrapping_mul(0x100000001b3);
    }
    hash
}

/// 生成节点唯一 ID
pub fn node_id(base_url: &str, name: &str) -> String {
    format!("{:016x}", fnv1a(&format!("{base_url}|{name}")))
}

/// 生成不可压缩的随机数据块(上传测速需真实随机数据,避免传输层优化干扰)
pub fn random_block(len: usize) -> Vec<u8> {
    use rand::{Rng, rng};
    let mut buf = vec![0u8; len];
    rng().fill_bytes(&mut buf);
    buf
}

/// 生成短随机 ID(测速结果 ID)
pub fn short_id() -> String {
    use rand::RngExt;
    let mut r = rand::rng();
    format!(
        "{:08x}{:08x}",
        r.random::<u32>(),
        chrono::Utc::now().timestamp_millis() as u32
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validate_url() {
        assert!(validate_server_url("https://speed.example.com/backend", false).is_ok());
        assert!(validate_server_url("http://speed.example.com", false).is_err());
        assert!(validate_server_url("http://speed.example.com", true).is_ok());
        assert!(validate_server_url("ftp://x", false).is_err());
        assert!(validate_server_url("not a url", false).is_err());
    }

    #[test]
    fn test_extract_host() {
        assert_eq!(
            extract_host("https://lon.speedtest.clouvider.net/backend/empty.php"),
            Some("lon.speedtest.clouvider.net".into())
        );
        assert_eq!(extract_host("https://speed.cloudflare.com/__down"), Some("speed.cloudflare.com".into()));
    }

    #[test]
    fn test_node_id_stable() {
        let a = node_id("https://a.com", "Node A");
        let b = node_id("https://a.com", "Node A");
        assert_eq!(a, b);
        assert_eq!(a.len(), 16);
    }

    #[test]
    fn test_random_block_not_constant() {
        let b1 = random_block(1024);
        let b2 = random_block(1024);
        assert_ne!(b1, b2); // 随机数据,不应重复
        assert_eq!(b1.len(), 1024);
    }
}

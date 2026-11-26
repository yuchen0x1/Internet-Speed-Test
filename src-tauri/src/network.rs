//! 网络客户端与代理策略。
//!
//! 所有测速请求都应通过 [`ProxyPolicy::client`] 创建客户端。代理配置在
//! 这里统一解析，避免某个适配器意外绕过用户的代理。代理 URL 只保存在
//! 进程内，错误文本不会包含用户名、密码或完整 URL。

use std::time::Duration;

use reqwest::{Client, ClientBuilder, Proxy, Url};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProxyMode {
    Auto,
    Direct,
    Manual,
}

impl ProxyMode {
    pub fn parse(value: &str) -> Self {
        match value.trim().to_ascii_lowercase().as_str() {
            "direct" | "none" | "off" => Self::Direct,
            "manual" | "custom" => Self::Manual,
            _ => Self::Auto,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Auto => "auto",
            Self::Direct => "direct",
            Self::Manual => "manual",
        }
    }
}

#[derive(Debug, Clone)]
struct ProxyEntry {
    scheme: Option<String>,
    url: Url,
}

/// 已解析的应用代理策略。
#[derive(Debug, Clone)]
pub struct ProxyPolicy {
    mode: ProxyMode,
    entries: Vec<ProxyEntry>,
    bypass: Vec<String>,
    unsupported: Option<String>,
}

impl Default for ProxyPolicy {
    fn default() -> Self {
        Self::from_parts("auto", "", "").unwrap_or_else(|_| Self {
            mode: ProxyMode::Auto,
            entries: Vec::new(),
            bypass: Vec::new(),
            unsupported: Some(
                "自动代理配置无效，请检查 HTTP_PROXY/HTTPS_PROXY/ALL_PROXY 或系统代理设置".into(),
            ),
        })
    }
}

impl ProxyPolicy {
    /// 从设置字段创建策略。`auto` 同时读取环境变量和 Windows Internet
    /// Settings；PAC/WPAD 只报告为不支持，不会静默降级为直连。
    pub fn from_parts(mode: &str, proxy_url: &str, bypass: &str) -> Result<Self, String> {
        let mode = ProxyMode::parse(mode);
        let bypass = parse_bypass(bypass);
        let mut policy = Self {
            mode,
            entries: Vec::new(),
            bypass,
            unsupported: None,
        };

        match mode {
            ProxyMode::Direct => {}
            ProxyMode::Manual => {
                let raw = proxy_url.trim();
                if raw.is_empty() {
                    return Err("手动代理模式需要填写代理地址".into());
                }
                policy.entries = parse_proxy_entries(raw)?;
            }
            ProxyMode::Auto => {
                policy.entries = env_proxy_entries()?;
                if let Ok(value) = std::env::var("NO_PROXY").or_else(|_| std::env::var("no_proxy"))
                {
                    policy.bypass.extend(parse_bypass(&value));
                }
                if policy.entries.is_empty() {
                    #[cfg(windows)]
                    {
                        let system = windows_proxy_entries()?;
                        policy.entries = system.entries;
                        policy.unsupported = system.unsupported;
                        policy.bypass.extend(system.bypass);
                    }
                }
            }
        }
        Ok(policy)
    }

    pub fn mode(&self) -> ProxyMode {
        self.mode
    }

    pub fn is_direct_route(&self) -> bool {
        self.entries.is_empty() && self.unsupported.is_none()
    }

    pub fn has_proxy(&self) -> bool {
        !self.entries.is_empty() || self.unsupported.is_some()
    }

    /// 为一个请求构造使用相同代理策略的 reqwest 客户端。
    pub fn client(&self, user_agent: &str, connect_timeout: Duration) -> Result<Client, String> {
        self.client_builder(user_agent, connect_timeout)?
            .build()
            .map_err(|e| format!("构建网络客户端失败: {}", redact_error(&e.to_string())))
    }

    pub fn client_builder(
        &self,
        user_agent: &str,
        connect_timeout: Duration,
    ) -> Result<ClientBuilder, String> {
        // 禁止 reqwest 在这里重新读取环境变量；下面的 entries 是本策略
        // 解析后的快照，保证所有适配器采用同一路由。
        let mut builder = Client::builder()
            .user_agent(user_agent)
            .connect_timeout(connect_timeout);
        if self.entries.is_empty() && self.unsupported.is_none() {
            builder = builder.no_proxy();
        }

        // PAC/WPAD needs an evaluator that is intentionally not bundled.
        // Route through a local black-hole proxy instead of silently falling
        // back to direct Internet access.
        if self.unsupported.is_some() {
            let blocked =
                Proxy::all("http://127.0.0.1:0").map_err(|_| "代理策略不可用".to_string())?;
            builder = builder.proxy(blocked);
        }

        for entry in &self.entries {
            let mut proxy = match entry.scheme.as_deref() {
                Some("http") => Proxy::http(entry.url.clone()),
                Some("https") => Proxy::https(entry.url.clone()),
                _ => Proxy::all(entry.url.clone()),
            }
            .map_err(|e| format!("代理地址无效: {}", redact_error(&e.to_string())))?;
            if !self.bypass.is_empty() {
                if let Some(no_proxy) = reqwest::NoProxy::from_string(&self.bypass.join(",")) {
                    proxy = proxy.no_proxy(Some(no_proxy));
                }
            }
            builder = builder.proxy(proxy);
        }
        Ok(builder)
    }

    /// 构造客户端时保留错误分类，但不暴露代理凭据。
    pub fn unavailable_reason(&self) -> Option<&str> {
        self.unsupported.as_deref()
    }

    /// WebSocket 适配器目前没有实现 HTTP CONNECT/PAC 代理隧道。
    /// 仅在确定该策略会经过代理时返回 false，避免 NDT7 偷偷直连。
    pub fn websocket_direct_allowed(&self) -> bool {
        self.is_direct_route()
    }

    #[cfg(test)]
    fn entry_urls(&self) -> Vec<String> {
        self.entries.iter().map(|e| e.url.to_string()).collect()
    }
}

fn parse_bypass(value: &str) -> Vec<String> {
    value
        .split(|c: char| c == ',' || c == ';' || c.is_whitespace())
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(ToOwned::to_owned)
        .collect()
}

fn parse_proxy_entries(raw: &str) -> Result<Vec<ProxyEntry>, String> {
    let mut entries = Vec::new();
    for part in raw.split(';').map(str::trim).filter(|s| !s.is_empty()) {
        let assignment = part
            .split_once('=')
            .filter(|(kind, _)| !kind.contains("://"));
        let (scheme, value) = if let Some((kind, value)) = assignment {
            let kind = kind.trim().to_ascii_lowercase();
            if !matches!(kind.as_str(), "http" | "https" | "socks4" | "socks5") {
                return Err("代理协议仅支持 http、https、socks4 或 socks5".into());
            }
            (Some(kind), value.trim())
        } else {
            (None, part)
        };
        let normalized = if value.contains("://") {
            value.to_string()
        } else {
            let protocol = match scheme.as_deref() {
                Some("socks4") => "socks4",
                Some("socks5") => "socks5",
                _ => "http",
            };
            format!("{protocol}://{value}")
        };
        let url = Url::parse(&normalized)
            .map_err(|_| "代理地址格式无效，请检查主机、端口和协议".to_string())?;
        if !matches!(url.scheme(), "http" | "https" | "socks4" | "socks5") {
            return Err("代理协议仅支持 http、https、socks4 或 socks5".into());
        }
        entries.push(ProxyEntry { scheme, url });
    }
    if entries.is_empty() {
        return Err("代理地址格式无效".into());
    }
    Ok(entries)
}

fn env_proxy_entries() -> Result<Vec<ProxyEntry>, String> {
    // 环境变量名大小写在 Windows 不敏感，显式读取两种写法以覆盖
    // Unix shell 与 Windows 用户环境。
    let mut values = Vec::new();
    for (kind, names) in [
        ("http", ["HTTP_PROXY", "http_proxy"]),
        ("https", ["HTTPS_PROXY", "https_proxy"]),
        ("all", ["ALL_PROXY", "all_proxy"]),
    ] {
        for name in names {
            if let Ok(value) = std::env::var(name) {
                if !value.trim().is_empty() {
                    values.push((kind, value));
                    break;
                }
            }
        }
    }

    let mut entries = Vec::new();
    for (kind, value) in values {
        let parsed = parse_proxy_entries(&value)?;
        for mut entry in parsed {
            if kind != "all" {
                entry.scheme = Some(kind.to_string());
            }
            entries.push(entry);
        }
    }
    Ok(dedup_entries(entries))
}

fn dedup_entries(entries: Vec<ProxyEntry>) -> Vec<ProxyEntry> {
    let mut out = Vec::new();
    for entry in entries {
        if !out.iter().any(|existing: &ProxyEntry| {
            existing.scheme == entry.scheme && existing.url == entry.url
        }) {
            out.push(entry);
        }
    }
    out
}

#[cfg(any(windows, test))]
fn windows_auto_proxy_configured(auto_detect: bool, auto_config_url: &str) -> bool {
    auto_detect || !auto_config_url.trim().is_empty()
}

pub(crate) fn redact_error(value: &str) -> String {
    // reqwest errors may echo a URL containing proxy credentials. Keep only a
    // stable category for logs and UI.
    if value.contains('@') || value.contains("://") {
        "网络请求失败".into()
    } else {
        value.to_string()
    }
}

#[cfg(windows)]
struct WindowsProxy {
    entries: Vec<ProxyEntry>,
    bypass: Vec<String>,
    unsupported: Option<String>,
}

#[cfg(windows)]
fn windows_proxy_entries() -> Result<WindowsProxy, String> {
    use winreg::enums::{HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE};
    use winreg::RegKey;

    let paths = [
        (RegKey::predef(HKEY_CURRENT_USER), "用户"),
        (RegKey::predef(HKEY_LOCAL_MACHINE), "系统"),
    ];
    for (root, _) in paths {
        let Ok(key) =
            root.open_subkey("Software\\Microsoft\\Windows\\CurrentVersion\\Internet Settings")
        else {
            continue;
        };
        let enabled: u32 = key.get_value("ProxyEnable").unwrap_or(0);
        let bypass = key
            .get_value::<String, _>("ProxyOverride")
            .unwrap_or_default();
        let pac = key
            .get_value::<String, _>("AutoConfigURL")
            .unwrap_or_default();
        let auto_detect = key.get_value::<u32, _>("AutoDetect").unwrap_or(0) != 0;
        if enabled != 0 {
            let server = key
                .get_value::<String, _>("ProxyServer")
                .map_err(|_| "Windows 系统代理已启用但未配置代理地址".to_string())?;
            return Ok(WindowsProxy {
                entries: parse_proxy_entries(&server)?,
                bypass: parse_bypass(&bypass),
                unsupported: None,
            });
        }
        if windows_auto_proxy_configured(auto_detect, &pac) {
            return Ok(WindowsProxy {
                entries: Vec::new(),
                bypass: parse_bypass(&bypass),
                unsupported: Some("Windows PAC/WPAD 代理暂不支持，请改用手动代理或直连".into()),
            });
        }
    }
    Ok(WindowsProxy {
        entries: Vec::new(),
        bypass: Vec::new(),
        unsupported: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn direct_mode_never_uses_environment() {
        let p = ProxyPolicy::from_parts("direct", "http://127.0.0.1:1", "").unwrap();
        assert!(p.is_direct_route());
        assert!(!p.has_proxy());
    }

    #[test]
    fn manual_proxy_is_parsed_without_exposing_credentials() {
        let p = ProxyPolicy::from_parts(
            "manual",
            "http://user:secret@127.0.0.1:8080",
            "localhost, 127.0.0.1",
        )
        .unwrap();
        assert!(!p.is_direct_route());
        assert_eq!(p.entry_urls(), vec!["http://user:secret@127.0.0.1:8080/"]);
        assert!(redact_error("proxy http://user:secret@host:1 failed") == "网络请求失败");
    }

    #[test]
    fn redact_error_hides_sensitive_url_query_parameters() {
        let error =
            "error sending request for url (https://node.example/speed?access_token=secret)";

        let redacted = redact_error(error);

        assert!(!redacted.contains("access_token"));
        assert!(!redacted.contains("secret"));
    }

    #[test]
    fn manual_proxy_requires_url() {
        assert!(ProxyPolicy::from_parts("manual", "", "").is_err());
        assert!(ProxyPolicy::from_parts("manual", "not a url", "").is_err());
    }

    #[test]
    fn proxy_server_list_supports_per_scheme_entries() {
        let entries = parse_proxy_entries("http=127.0.0.1:8080;https=127.0.0.1:8443").unwrap();
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].scheme.as_deref(), Some("http"));
        assert_eq!(entries[1].scheme.as_deref(), Some("https"));
    }

    #[test]
    fn proxy_credentials_can_contain_equals() {
        let entries = parse_proxy_entries("http://user:p=ss@127.0.0.1:8080").unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].url.password(), Some("p=ss"));
        assert_eq!(entries[0].scheme, None);
    }

    #[test]
    fn socks_assignment_preserves_the_proxy_protocol() {
        let entries = parse_proxy_entries("socks5=127.0.0.1:1080").unwrap();
        assert_eq!(entries[0].url.scheme(), "socks5");
        let entries = parse_proxy_entries("socks4=127.0.0.1:1081").unwrap();
        assert_eq!(entries[0].url.scheme(), "socks4");
    }

    #[test]
    fn windows_wpad_and_pac_are_detected_as_unsupported_auto_proxy() {
        assert!(windows_auto_proxy_configured(true, ""));
        assert!(windows_auto_proxy_configured(
            false,
            " https://proxy.example/proxy.pac "
        ));
        assert!(!windows_auto_proxy_configured(false, "  "));
    }
}

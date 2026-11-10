//! 网络诊断层:本地接口、公网信息、DNS、网关、TCP/HTTPS、ICMP、路由追踪。
//!
//! 安全与诚实性原则:
//! - 所有子进程参数均为常量白名单 + 主机名合法性校验(不存在命令注入面)。
//! - 无法检测的项目返回 unknown,不伪装成功。
//! - 公网信息仅在用户授权(show_public_ip)时获取。

use std::net::IpAddr;
use std::time::{Duration, Instant};

use serde::Serialize;

use crate::engine::quality::{
    jitter_ms as calc_jitter, parse_ping_output, parse_traceroute_output, PingOutput, TraceHop,
};

const CREATE_NO_WINDOW: u32 = 0x0800_0000;

// ============================================================
// 本地网络接口
// ============================================================

#[derive(Debug, Clone, Serialize)]
pub struct IfaceInfo {
    pub name: String,
    pub friendly_name: Option<String>,
    pub description: Option<String>,
    /// wifi | ethernet | other
    pub kind: String,
    pub mac: Option<String>,
    pub ipv4: Vec<String>,
    pub ipv6: Vec<String>,
    pub gateway: Option<String>,
    pub dns_servers: Vec<String>,
    pub oper_up: bool,
    /// 网卡协商链路速度(Mbps),来自系统 API,与宽带速率无关
    pub link_mbps: Option<u64>,
    pub mtu: Option<u32>,
    pub is_default: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct LocalNetInfo {
    pub interfaces: Vec<IfaceInfo>,
    pub hostname: String,
    pub os: String,
}

fn iface_kind_str(t: &netdev::interface::types::InterfaceType) -> &'static str {
    use netdev::interface::types::InterfaceType as T;
    match t {
        T::Wireless80211 | T::PeerToPeerWireless => "wifi",
        T::Ethernet
        | T::GigabitEthernet
        | T::FastEthernetT
        | T::FastEthernetFx
        | T::Ethernet3Megabit => "ethernet",
        T::Wwan | T::Wwanpp | T::Wwanpp2 | T::Wman => "cellular",
        T::Loopback => "loopback",
        _ => "other",
    }
}

/// 采集本机全部有意义的网络接口
pub fn collect_interfaces() -> Vec<IfaceInfo> {
    let mut out = Vec::new();
    let ifaces = netdev::get_interfaces();
    for i in ifaces {
        if i.if_type == netdev::interface::types::InterfaceType::Loopback {
            continue;
        }
        let oper_up = matches!(i.oper_state, netdev::interface::state::OperState::Up);
        let has_addr = !i.ipv4.is_empty() || !i.ipv6.is_empty();
        if !oper_up && !has_addr {
            continue;
        }
        let gateway = i.gateway.as_ref().and_then(|g| {
            g.ipv4
                .first()
                .map(|a| a.to_string())
                .or_else(|| g.ipv6.first().map(|a| a.to_string()))
        });
        out.push(IfaceInfo {
            name: i.name.clone(),
            friendly_name: i.friendly_name.clone(),
            description: i.description.clone(),
            kind: iface_kind_str(&i.if_type).to_string(),
            mac: i.mac_addr.map(|m| m.to_string()),
            ipv4: i.ipv4.iter().map(|n| n.to_string()).collect(),
            ipv6: i.ipv6.iter().map(|n| n.to_string()).collect(),
            gateway,
            dns_servers: i.dns_servers.iter().map(|d| d.to_string()).collect(),
            oper_up,
            link_mbps: i.transmit_speed.map(|s| s / 1_000_000),
            mtu: i.mtu,
            is_default: i.default,
        });
    }
    out
}

/// 标题栏/状态栏使用的轻量网络状态
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct NetStatus {
    pub connected: bool,
    pub kind: String,
    pub iface: String,
    pub link_mbps: Option<u64>,
    pub ipv4: Option<String>,
    pub has_ipv6: bool,
}

pub fn current_net_status() -> NetStatus {
    let ifaces = collect_interfaces();
    let def = ifaces.iter().find(|i| i.is_default);
    match def {
        Some(i) => NetStatus {
            connected: i.oper_up && i.gateway.is_some(),
            kind: i.kind.clone(),
            iface: i.friendly_name.clone().unwrap_or_else(|| i.name.clone()),
            link_mbps: i.link_mbps,
            ipv4: i
                .ipv4
                .first()
                .map(|s| s.split('/').next().unwrap_or(s).to_string()),
            has_ipv6: !i.ipv6.is_empty(),
        },
        None => NetStatus {
            connected: false,
            kind: "none".into(),
            iface: String::new(),
            link_mbps: None,
            ipv4: None,
            has_ipv6: false,
        },
    }
}

// ============================================================
// Wi-Fi 详情(netsh,仅诊断页按需调用)
// ============================================================

#[derive(Debug, Clone, Serialize)]
pub struct WifiDetails {
    pub ssid: Option<String>,
    pub bssid: Option<String>,
    /// 0-100
    pub signal_percent: Option<u32>,
    pub radio_type: Option<String>,
    pub channel: Option<String>,
    pub receive_rate_mbps: Option<f64>,
    pub transmit_rate_mbps: Option<f64>,
}

pub fn valid_host(host: &str) -> bool {
    !host.is_empty()
        && host.len() <= 253
        && host
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '-' || c == ':')
}

pub async fn wifi_details() -> Option<WifiDetails> {
    use std::process::Command;
    use std::process::Stdio;
    let out = tokio::task::spawn_blocking(move || {
        #[cfg(target_os = "windows")]
        {
            let mut cmd = Command::new("netsh");
            cmd.args(["wlan", "show", "interfaces"])
                .stdout(Stdio::piped())
                .stderr(Stdio::null());
            #[cfg(target_os = "windows")]
            {
                use std::os::windows::process::CommandExt;
                cmd.creation_flags(CREATE_NO_WINDOW);
            }
            cmd.output().ok()
        }
        #[cfg(not(target_os = "windows"))]
        {
            Command::new("true").output().ok()
        }
    })
    .await
    .ok()??;
    let text = String::from_utf8_lossy(&out.stdout);
    parse_wifi_details(&text)
}

/// 解析 `netsh wlan show interfaces` 文本输出(纯函数,便于测试)
fn parse_wifi_details(text: &str) -> Option<WifiDetails> {
    let mut d = WifiDetails {
        ssid: None,
        bssid: None,
        signal_percent: None,
        radio_type: None,
        channel: None,
        receive_rate_mbps: None,
        transmit_rate_mbps: None,
    };
    for line in text.lines() {
        let t = line.trim();
        // BSSID 前缀更长,必须先于 SSID 匹配
        if let Some(v) = t.strip_prefix("BSSID") {
            if let Some(val) = v.trim_start().strip_prefix(':') {
                d.bssid = Some(val.trim().to_string());
            }
            continue;
        }
        if let Some(v) = t.strip_prefix("SSID") {
            if let Some(val) = v.trim_start().strip_prefix(':') {
                d.ssid = Some(val.trim().to_string());
            }
            continue;
        }
        if t.contains('%') && (t.contains("信号") || t.to_lowercase().contains("signal")) {
            d.signal_percent = t
                .split('%')
                .next()
                .and_then(|seg| seg.rsplit(|c: char| !c.is_ascii_digit()).next())
                .and_then(|n| n.parse::<u32>().ok());
        } else if t.contains("无线电类型") || t.to_lowercase().contains("radio type") {
            d.radio_type = t.split(':').nth(1).map(|s| s.trim().to_string());
        } else if t.contains("信道") || t.to_lowercase().contains("channel") {
            d.channel = t.split(':').nth(1).map(|s| s.trim().to_string());
        } else if t.contains("接收速率") || t.to_lowercase().contains("receive rate") {
            d.receive_rate_mbps = t
                .split(':')
                .nth(1)
                .and_then(|s| {
                    s.trim()
                        .split(|c: char| !c.is_ascii_digit() && c != '.')
                        .next()
                })
                .and_then(|n| n.parse::<f64>().ok());
        } else if t.contains("传输速率") || t.to_lowercase().contains("transmit rate") {
            d.transmit_rate_mbps = t
                .split(':')
                .nth(1)
                .and_then(|s| {
                    s.trim()
                        .split(|c: char| !c.is_ascii_digit() && c != '.')
                        .next()
                })
                .and_then(|n| n.parse::<f64>().ok());
        }
    }
    if d.ssid.is_none() && d.signal_percent.is_none() {
        // 无线网卡不存在或未连接
        return None;
    }
    Some(d)
}

// ============================================================
// ICMP:首选系统 API(IcmpSendEcho,免管理员、无子进程、毫秒级精度),
//      失败时回退系统 ping 子进程(语言无关解析)。
// ============================================================

/// ICMP 探测载荷(标识用途,长度 32 字节)
const ICMP_PAYLOAD: &[u8] = b"NetVisionPro-ICMP-Probe-Payload!";

#[cfg(windows)]
mod icmp_api {
    use windows_sys::Win32::NetworkManagement::IpHelper::{
        IcmpCloseHandle, IcmpCreateFile, IcmpSendEcho, ICMP_ECHO_REPLY, IP_SUCCESS,
    };

    /// ICMP 会话句柄(IcmpCreateFile / IcmpCloseHandle)
    pub struct IcmpSession(windows_sys::Win32::Foundation::HANDLE);
    unsafe impl Send for IcmpSession {}

    impl IcmpSession {
        pub fn new() -> Option<Self> {
            let h = unsafe { IcmpCreateFile() };
            (!h.is_null()).then_some(Self(h))
        }

        /// 单次探测:Some(rtt_ms) 成功;None 超时或不可达
        pub fn ping_once(&self, dest: u32, payload: &[u8], timeout_ms: u32) -> Option<u32> {
            let mut reply_buf = vec![0u8; payload.len() + 256];
            let replies = unsafe {
                IcmpSendEcho(
                    self.0,
                    dest,
                    payload.as_ptr() as *const core::ffi::c_void,
                    payload.len() as u16,
                    std::ptr::null(),
                    reply_buf.as_mut_ptr() as *mut core::ffi::c_void,
                    reply_buf.len() as u32,
                    timeout_ms,
                )
            };
            if replies == 0 {
                return None;
            }
            let reply = unsafe { &*(reply_buf.as_ptr() as *const ICMP_ECHO_REPLY) };
            if reply.Status == IP_SUCCESS {
                // 回环等场景 RTT 可能为 0(亚毫秒),属真实测量
                Some(reply.RoundTripTime)
            } else {
                None
            }
        }
    }

    impl Drop for IcmpSession {
        fn drop(&mut self) {
            unsafe { IcmpCloseHandle(self.0) };
        }
    }
}

pub async fn icmp_ping(host: &str, count: u32, timeout_ms: u32) -> PingOutput {
    let count = count.clamp(1, 64);
    if !valid_host(host) {
        return PingOutput {
            loss_percent: None,
            rtts_ms: vec![],
            avg_ms: None,
        };
    }
    // 首选:系统 ICMP API(仅 IPv4;IPv6 目标走子进程兜底)
    #[cfg(windows)]
    if let Some(out) = icmp_ping_api(host, count, timeout_ms).await {
        return out;
    }
    icmp_ping_subprocess(host, count, timeout_ms).await
}

/// IcmpSendEcho 实现(同步 API 放入阻塞线程池)
#[cfg(windows)]
async fn icmp_ping_api(host: &str, count: u32, timeout_ms: u32) -> Option<PingOutput> {
    let host = host.to_string();
    tokio::task::spawn_blocking(move || {
        use std::net::ToSocketAddrs;
        let Ok(mut addrs) = (host.as_str(), 0u16).to_socket_addrs() else {
            return None;
        };
        let Some(v4) = addrs.find_map(|a| match a.ip() {
            std::net::IpAddr::V4(v) => Some(v),
            std::net::IpAddr::V6(_) => None,
        }) else {
            return None; // 无 IPv4 记录 → 回退子进程路径
        };
        let session = icmp_api::IcmpSession::new()?;
        // IPAddr 要求内存字节为网络序(a.b.c.d);Windows 小端平台上等价于 from_le_bytes
        let dest = u32::from_le_bytes(v4.octets());
        let mut rtts: Vec<f64> = Vec::with_capacity(count as usize);
        let mut lost: u32 = 0;
        for i in 0..count {
            if i > 0 {
                std::thread::sleep(Duration::from_millis(20));
            }
            match session.ping_once(dest, ICMP_PAYLOAD, timeout_ms) {
                Some(rtt) => rtts.push(rtt as f64),
                None => lost += 1,
            }
        }
        let loss_percent = lost as f64 * 100.0 / count as f64;
        let avg_ms = (!rtts.is_empty()).then(|| rtts.iter().sum::<f64>() / rtts.len() as f64);
        Some(PingOutput {
            loss_percent: Some(loss_percent),
            rtts_ms: rtts,
            avg_ms,
        })
    })
    .await
    .ok()
    .flatten()
}

#[cfg(not(windows))]
async fn icmp_ping_api(_host: &str, _count: u32, _timeout_ms: u32) -> Option<PingOutput> {
    None
}

/// 兜底:系统 ping 子进程(覆盖 IPv6 与 API 不可用场景)
async fn icmp_ping_subprocess(host: &str, count: u32, timeout_ms: u32) -> PingOutput {
    let host = host.to_string();
    let out = tokio::process::Command::new("ping")
        .args([
            "-n",
            &count.to_string(),
            "-w",
            &timeout_ms.to_string(),
            &host,
        ])
        .creation_flags(CREATE_NO_WINDOW)
        .output()
        .await;
    match out {
        Ok(o) => parse_ping_output(&String::from_utf8_lossy(&o.stdout)),
        Err(_) => PingOutput {
            loss_percent: None,
            rtts_ms: vec![],
            avg_ms: None,
        },
    }
}

/// 路由追踪:逐行解析并收集;语言无关
pub async fn traceroute(host: &str, max_hops: u32) -> Result<Vec<TraceHop>, String> {
    if !valid_host(host) {
        return Err("无效的主机名".into());
    }
    let out = tokio::process::Command::new("tracert")
        .args(["-d", "-h", &max_hops.to_string(), "-w", "900", host])
        .creation_flags(CREATE_NO_WINDOW)
        .output()
        .await
        .map_err(|e| e.to_string())?;
    if !out.status.success() && out.stdout.is_empty() {
        return Err("路由追踪启动失败".into());
    }
    Ok(parse_traceroute_output(&String::from_utf8_lossy(
        &out.stdout,
    )))
}

// ============================================================
// 原始 DNS 探测(UDP,自实现协议构造与解析,零第三方依赖)
// ============================================================

/// 构造标准 A 记录查询报文。
/// 按 RFC 1035 校验:单标签 ≤63 字节、QNAME 线上格式 ≤255 字节;
/// 超限返回 None,拒绝发送畸形报文(而不是 `len as u8` 截断出错误长度)。
pub fn build_dns_query(domain: &str, id: u16) -> Option<Vec<u8>> {
    let mut p = Vec::with_capacity(12 + domain.len() + 6);
    p.extend_from_slice(&id.to_be_bytes()); // ID
    p.extend_from_slice(&0x0100u16.to_be_bytes()); // Flags: RD=1
    p.extend_from_slice(&1u16.to_be_bytes()); // QDCOUNT
    p.extend_from_slice(&0u16.to_be_bytes()); // ANCOUNT
    p.extend_from_slice(&0u16.to_be_bytes()); // NSCOUNT
    p.extend_from_slice(&0u16.to_be_bytes()); // ARCOUNT
    let mut qname_len = 0usize; // QNAME 线上长度(各长度字节+标签+根0)
    for label in domain.split('.') {
        if label.is_empty() {
            continue;
        }
        if label.len() > 63 {
            return None;
        }
        qname_len += 1 + label.len();
        p.push(label.len() as u8);
        p.extend_from_slice(label.as_bytes());
    }
    qname_len += 1; // 根标签
    if qname_len > 255 {
        return None;
    }
    p.push(0);
    p.extend_from_slice(&1u16.to_be_bytes()); // QTYPE = A
    p.extend_from_slice(&1u16.to_be_bytes()); // QCLASS = IN
    Some(p)
}

/// 解析 DNS 响应:(id, ancount, 第一个 A 记录)
pub fn parse_dns_reply(buf: &[u8]) -> Option<(u16, u16, Option<std::net::Ipv4Addr>)> {
    if buf.len() < 12 {
        return None;
    }
    let id = u16::from_be_bytes([buf[0], buf[1]]);
    let ancount = u16::from_be_bytes([buf[6], buf[7]]);
    // 跳过 Question 区(QNAME + QTYPE + QCLASS)
    let mut pos = 12;
    loop {
        if pos >= buf.len() {
            return None;
        }
        let len = buf[pos];
        if len == 0 {
            pos += 1;
            break;
        }
        if len & 0xC0 != 0 {
            pos += 2; // 压缩指针(异常场景)
            break;
        }
        pos += 1 + len as usize;
    }
    pos += 4;
    // 遍历 Answer 区找第一个 A 记录
    for _ in 0..ancount {
        // 名称
        loop {
            if pos >= buf.len() {
                return Some((id, ancount, None));
            }
            let len = buf[pos];
            if len == 0 {
                pos += 1;
                break;
            }
            if len & 0xC0 != 0 {
                pos += 2;
                break;
            }
            pos += 1 + len as usize;
        }
        if pos + 10 > buf.len() {
            return Some((id, ancount, None));
        }
        let rtype = u16::from_be_bytes([buf[pos], buf[pos + 1]]);
        let rdlength = u16::from_be_bytes([buf[pos + 8], buf[pos + 9]]) as usize;
        pos += 10;
        if rtype == 1 && rdlength == 4 && pos + 4 <= buf.len() {
            let a = std::net::Ipv4Addr::new(buf[pos], buf[pos + 1], buf[pos + 2], buf[pos + 3]);
            return Some((id, ancount, Some(a)));
        }
        pos += rdlength;
    }
    Some((id, ancount, None))
}

#[derive(Debug, Clone, Serialize)]
pub struct DnsServerProbe {
    pub server: String,
    pub ok: bool,
    pub rtt_ms: Option<f64>,
    pub answer_count: u16,
    pub resolved_ip: Option<String>,
    pub error: Option<String>,
}

/// 向指定 DNS 服务器直接发起 UDP A 查询
pub async fn dns_udp_probe(server: IpAddr, domain: &str) -> DnsServerProbe {
    let id: u16 = ((std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.subsec_nanos())
        .unwrap_or(0)
        & 0xFFFF) as u16)
        | 1;
    let base = DnsServerProbe {
        server: server.to_string(),
        ok: false,
        rtt_ms: None,
        answer_count: 0,
        resolved_ip: None,
        error: None,
    };
    let Some(packet) = build_dns_query(domain, id) else {
        return DnsServerProbe {
            error: Some("域名不符合 DNS 命名规则(标签超过 63 字节或名称过长)".into()),
            ..base
        };
    };
    let req = tokio::time::timeout(Duration::from_millis(2500), async move {
        let bind = if server.is_ipv4() {
            "0.0.0.0:0"
        } else {
            "[::]:0"
        };
        let sock = tokio::net::UdpSocket::bind(bind).await?;
        sock.connect((server, 53)).await?;
        let t0 = Instant::now();
        sock.send(&packet).await?;
        let mut buf = [0u8; 512];
        let n = sock.recv(&mut buf).await?;
        let rtt = t0.elapsed().as_secs_f64() * 1000.0;
        Ok::<_, std::io::Error>((rtt, buf[..n].to_vec()))
    })
    .await;
    match req {
        Err(_) => DnsServerProbe {
            error: Some("查询超时(2.5s)".into()),
            ..base
        },
        Ok(Err(e)) => DnsServerProbe {
            error: Some(format!("UDP 查询失败: {e}")),
            ..base
        },
        Ok(Ok((rtt, buf))) => match parse_dns_reply(&buf) {
            Some((rid, ancount, a)) if rid == id => {
                if ancount > 0 {
                    DnsServerProbe {
                        ok: true,
                        rtt_ms: Some(rtt),
                        answer_count: ancount,
                        resolved_ip: a.map(|ip| ip.to_string()),
                        error: None,
                        ..base
                    }
                } else {
                    DnsServerProbe {
                        ok: true,
                        rtt_ms: Some(rtt),
                        answer_count: 0,
                        error: Some("服务器响应但无 A 记录".into()),
                        ..base
                    }
                }
            }
            _ => DnsServerProbe {
                error: Some("响应格式异常".into()),
                ..base
            },
        },
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct DnsDomainResult {
    pub domain: String,
    pub ok: bool,
    pub ms: Option<f64>,
    pub ips: Vec<String>,
    pub error: Option<String>,
}

/// 系统解析器逐域名测速
async fn system_resolve(domain: &str) -> DnsDomainResult {
    let t0 = Instant::now();
    let res = tokio::time::timeout(
        Duration::from_millis(4000),
        tokio::net::lookup_host((domain, 443u16)),
    )
    .await;
    match res {
        Ok(Ok(iter)) => {
            let ms = t0.elapsed().as_secs_f64() * 1000.0;
            let ips: Vec<String> = iter.map(|a| a.ip().to_string()).take(4).collect();
            DnsDomainResult {
                domain: domain.into(),
                ok: !ips.is_empty(),
                ms: Some(ms),
                ips,
                error: None,
            }
        }
        Ok(Err(e)) => DnsDomainResult {
            domain: domain.into(),
            ok: false,
            ms: None,
            ips: vec![],
            error: Some(format!("{e}")),
        },
        Err(_) => DnsDomainResult {
            domain: domain.into(),
            ok: false,
            ms: None,
            ips: vec![],
            error: Some("解析超时(4s)".into()),
        },
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct DnsTestReport {
    pub system_servers: Vec<String>,
    pub domains: Vec<DnsDomainResult>,
    pub server_probes: Vec<DnsServerProbe>,
}

/// DNS 综合检测:系统解析 + 对每个配置的 DNS 服务器直接探测
pub async fn dns_test() -> DnsTestReport {
    let ifaces = collect_interfaces();
    let servers: Vec<String> = ifaces
        .iter()
        .find(|i| i.is_default)
        .map(|i| i.dns_servers.clone())
        .unwrap_or_default();
    let domains = ["www.cloudflare.com", "www.baidu.com", "github.com"];
    let mut report = DnsTestReport {
        system_servers: servers.clone(),
        domains: Vec::new(),
        server_probes: Vec::new(),
    };
    for d in domains {
        report.domains.push(system_resolve(d).await);
    }
    let mut probes = Vec::new();
    for s in servers.iter().take(4) {
        if let Ok(ip) = s.parse::<IpAddr>() {
            probes.push(dns_udp_probe(ip, "www.cloudflare.com").await);
        }
    }
    // 系统 DNS 全不可达时,附加公共 DNS 对照(帮助定位是 DNS 还是网络问题)
    if probes.iter().all(|p| !p.ok) {
        for s in ["223.5.5.5", "8.8.8.8"] {
            if let Ok(ip) = s.parse() {
                probes.push(dns_udp_probe(ip, "www.cloudflare.com").await);
            }
        }
        if probes.iter().any(|p| p.ok) {
            report.domains.push(DnsDomainResult {
                domain: "__hint__".into(),
                ok: true,
                ms: None,
                ips: vec![],
                error: Some(
                    "系统配置的 DNS 服务器均无响应,但公共 DNS 可用:建议检查路由器/运营商 DNS"
                        .into(),
                ),
            });
        }
    }
    report.server_probes = probes;
    report
}

// ============================================================
// 网关 / TCP / HTTPS / IPv6
// ============================================================

#[derive(Debug, Clone, Serialize)]
pub struct GatewayReport {
    pub gateway: Option<String>,
    pub reachable: bool,
    pub loss_percent: Option<f64>,
    pub avg_ms: Option<f64>,
    pub note: Option<String>,
}

pub async fn gateway_test() -> GatewayReport {
    let ifaces = collect_interfaces();
    let gw = ifaces
        .iter()
        .find(|i| i.is_default)
        .and_then(|i| i.gateway.clone());
    match gw {
        Some(g) => {
            let p = icmp_ping(&g, 4, 1500).await;
            let reachable = p.loss_percent.is_some_and(|l| l < 100.0);
            GatewayReport {
                gateway: Some(g),
                reachable,
                loss_percent: p.loss_percent,
                avg_ms: p.avg_ms,
                note: if p.loss_percent.is_none() {
                    Some("网关禁用了 ICMP 响应,连通性未知".into())
                } else {
                    None
                },
            }
        }
        None => GatewayReport {
            gateway: None,
            reachable: false,
            loss_percent: None,
            avg_ms: None,
            note: Some("未找到默认网关(可能未连接任何网络)".into()),
        },
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct TcpResult {
    pub target: String,
    pub port: u16,
    pub ok: bool,
    pub ms: Option<f64>,
    pub error: Option<String>,
}

/// TCP 连接检测(真实 connect + 计时)
pub async fn tcp_connect_test(host: &str, port: u16) -> TcpResult {
    let t0 = Instant::now();
    let res = tokio::time::timeout(Duration::from_millis(4000), async {
        tokio::net::TcpStream::connect((host, port)).await
    })
    .await;
    match res {
        Ok(Ok(_)) => TcpResult {
            target: host.into(),
            port,
            ok: true,
            ms: Some(t0.elapsed().as_secs_f64() * 1000.0),
            error: None,
        },
        Ok(Err(e)) => TcpResult {
            target: host.into(),
            port,
            ok: false,
            ms: None,
            error: Some(format!("{e}")),
        },
        Err(_) => TcpResult {
            target: host.into(),
            port,
            ok: false,
            ms: None,
            error: Some("连接超时(4s)".into()),
        },
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct HttpsResult {
    pub url: String,
    pub ok: bool,
    pub ms: Option<f64>,
    pub status: Option<u16>,
    pub error: Option<String>,
}

pub async fn https_test(url: &str) -> HttpsResult {
    let client = reqwest::Client::builder()
        .user_agent(format!("NetVisionPro/{}", env!("CARGO_PKG_VERSION")))
        .connect_timeout(Duration::from_secs(5))
        .timeout(Duration::from_secs(8))
        .no_proxy()
        .build();
    let base = HttpsResult {
        url: url.into(),
        ok: false,
        ms: None,
        status: None,
        error: None,
    };
    match client {
        Err(e) => HttpsResult {
            error: Some(format!("客户端初始化失败: {e}")),
            ..base
        },
        Ok(c) => {
            let t0 = Instant::now();
            match c.get(url).send().await {
                Ok(r) => HttpsResult {
                    ok: r.status().is_success(),
                    ms: Some(t0.elapsed().as_secs_f64() * 1000.0),
                    status: Some(r.status().as_u16()),
                    ..base
                },
                Err(e) => HttpsResult {
                    error: Some(format!("{e}")),
                    ..base
                },
            }
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct IpVersionCheck {
    pub available: bool,
    /// 仅在 show_public_ip 授权时返回
    pub public_address: Option<String>,
    pub error: Option<String>,
}

fn plain_client() -> Result<reqwest::Client, String> {
    reqwest::Client::builder()
        .user_agent(format!("NetVisionPro/{}", env!("CARGO_PKG_VERSION")))
        .connect_timeout(Duration::from_secs(4))
        .timeout(Duration::from_secs(6))
        .no_proxy()
        .build()
        .map_err(|e| e.to_string())
}

async fn fetch_first_line(url: &str) -> Result<String, String> {
    let c = plain_client()?;
    let r = c.get(url).send().await.map_err(|e| e.to_string())?;
    if !r.status().is_success() {
        return Err(format!("HTTP {}", r.status().as_u16()));
    }
    let t = r.text().await.map_err(|e| e.to_string())?;
    Ok(t.lines().next().unwrap_or("").trim().to_string())
}

/// IPv6 可用性(独立于授权:始终检测;地址受授权控制)
pub async fn ipv6_check(show_public_ip: bool) -> IpVersionCheck {
    match tokio::time::timeout(
        Duration::from_secs(7),
        fetch_first_line("https://ipv6.icanhazip.com"),
    )
    .await
    {
        Ok(Ok(addr)) if !addr.is_empty() => IpVersionCheck {
            available: true,
            public_address: show_public_ip.then_some(addr),
            error: None,
        },
        Ok(Err(e)) => IpVersionCheck {
            available: false,
            public_address: None,
            error: Some(e),
        },
        Ok(Ok(_)) | Err(_) => IpVersionCheck {
            available: false,
            public_address: None,
            error: Some("IPv6 连接超时或不可用".into()),
        },
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct PublicInfo {
    pub allowed: bool,
    pub ipv4: Option<String>,
    pub ipv6: Option<String>,
    pub isp: Option<String>,
    pub edge_colo: Option<String>,
    pub country: Option<String>,
}

/// 公网信息:仅使用 Cloudflare 系端点(与测速流量同源,隐私策略一致)
pub async fn public_info(show_public_ip: bool) -> PublicInfo {
    if !show_public_ip {
        return PublicInfo {
            allowed: false,
            ipv4: None,
            ipv6: None,
            isp: None,
            edge_colo: None,
            country: None,
        };
    }
    let mut info = PublicInfo {
        allowed: true,
        ipv4: None,
        ipv6: None,
        isp: None,
        edge_colo: None,
        country: None,
    };
    // trace: ip / colo / loc(完整多行抓取)
    if let Ok(c) = plain_client() {
        if let Ok(r) = c
            .get("https://speed.cloudflare.com/cdn-cgi/trace")
            .timeout(Duration::from_secs(6))
            .send()
            .await
        {
            if let Ok(text) = r.text().await {
                for line in text.lines() {
                    if let Some(v) = line.strip_prefix("ip=") {
                        info.ipv4 = Some(v.trim().to_string());
                    } else if let Some(v) = line.strip_prefix("colo=") {
                        info.edge_colo = Some(v.trim().to_string());
                    } else if let Some(v) = line.strip_prefix("loc=") {
                        info.country = Some(v.trim().to_string());
                    }
                }
            }
        }
    }
    // IPv6
    if let Ok(addr) = fetch_first_line("https://ipv6.icanhazip.com").await {
        if addr.contains(':') {
            info.ipv6 = Some(addr);
        }
    }
    info
}

// ============================================================
// 连通性矩阵(MyIP 式):并发探测常用服务可达性
// ============================================================

#[derive(Debug, Clone, Serialize)]
pub struct ConnectivityEntry {
    pub name: String,
    pub host: String,
    pub ok: bool,
    pub ms: Option<f64>,
    pub error: Option<String>,
}

/// 常用服务连通性矩阵(TCP 443 并发探测)。
/// 注意:无法连接可能源于本地网络策略,不代表服务本身不可用。
pub async fn connectivity_grid() -> Vec<ConnectivityEntry> {
    use futures_util::StreamExt;
    let targets: Vec<(String, String, u16)> = vec![
        ("Cloudflare".into(), "speed.cloudflare.com".into(), 443),
        ("GitHub".into(), "github.com".into(), 443),
        ("百度".into(), "www.baidu.com".into(), 443),
        ("哔哩哔哩".into(), "www.bilibili.com".into(), 443),
        ("腾讯".into(), "www.qq.com".into(), 443),
        ("阿里云".into(), "www.aliyun.com".into(), 443),
        ("Google".into(), "www.google.com".into(), 443),
        ("Telegram".into(), "web.telegram.org".into(), 443),
    ];
    let stream = futures_util::stream::iter(targets)
        .map(|(name, host, port)| async move {
            let r = tcp_connect_test(&host, port).await;
            ConnectivityEntry {
                name,
                host,
                ok: r.ok,
                ms: r.ms,
                error: r.error,
            }
        })
        .buffer_unordered(8);
    let mut out: Vec<ConnectivityEntry> = stream.collect::<Vec<_>>().await;
    out.sort_by(|a, b| a.name.cmp(&b.name));
    out
}

// ============================================================
// 路由质量分析(MTR 式):逐跳真实 ICMP,输出每跳丢包/延迟/抖动
// ============================================================

#[derive(Debug, Clone, Serialize)]
pub struct HopQuality {
    pub hop: u32,
    pub ip: Option<String>,
    pub loss_percent: Option<f64>,
    pub avg_ms: Option<f64>,
    pub min_ms: Option<f64>,
    pub max_ms: Option<f64>,
    pub jitter_ms: Option<f64>,
    pub samples: u32,
    pub timeout: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct RouteQualityReport {
    pub host: String,
    pub hops: Vec<HopQuality>,
}

/// 逐跳探测:tracert 获取路径后,对每个可达跳发送真实 ICMP。
/// 串行执行(避免探测自身压垮线路),总耗时 ≈ 跳数 × 包数 × RTT。
pub async fn route_quality(
    host: &str,
    max_hops: u32,
    probes: u32,
) -> Result<RouteQualityReport, String> {
    let probes = probes.clamp(2, 20);
    let hops = traceroute(host, max_hops.clamp(3, 30)).await?;
    let mut out = Vec::with_capacity(hops.len());
    for h in &hops {
        match (&h.ip, h.timeout) {
            (Some(ip), false) => {
                let p = icmp_ping(ip, probes, 1200).await;
                let r = p.rtts_ms.clone();
                let (avg, min, max, jit) = if r.is_empty() {
                    (None, None, None, None)
                } else {
                    let avg = r.iter().sum::<f64>() / r.len() as f64;
                    let min = r.iter().cloned().fold(f64::MAX, f64::min);
                    let max = r.iter().cloned().fold(0.0, f64::max);
                    (Some(avg), Some(min), Some(max), Some(calc_jitter(&r)))
                };
                out.push(HopQuality {
                    hop: h.hop,
                    ip: Some(ip.clone()),
                    loss_percent: p.loss_percent,
                    avg_ms: avg,
                    min_ms: min,
                    max_ms: max,
                    jitter_ms: jit,
                    samples: r.len() as u32,
                    timeout: false,
                });
            }
            _ => out.push(HopQuality {
                hop: h.hop,
                ip: None,
                loss_percent: None,
                avg_ms: None,
                min_ms: None,
                max_ms: None,
                jitter_ms: None,
                samples: 0,
                timeout: true,
            }),
        }
    }
    Ok(RouteQualityReport {
        host: host.into(),
        hops: out,
    })
}

// ============================================================
// DNS 一致性 / 劫持检测:系统 DNS vs 多家公共 DNS 交叉验证
// ============================================================

#[derive(Debug, Clone, Serialize)]
pub struct DnsConsistencyEntry {
    pub server: String,
    /// 系统 | 公共
    pub source: String,
    pub ok: bool,
    pub rtt_ms: Option<f64>,
    pub ips: Vec<String>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct DnsConsistencyReport {
    pub domain: String,
    pub entries: Vec<DnsConsistencyEntry>,
    /// 一致 | 可疑劫持 | 不一致(可能地域调度) | 异常 | 未知
    pub verdict: String,
    pub note: Option<String>,
}

/// 保留/内网地址判定(命中即强烈提示劫持或 captive portal)
pub fn is_bogon_ipv4(ip: std::net::Ipv4Addr) -> bool {
    let [a, b, c, _d] = ip.octets();
    a == 0
        || a == 10
        || a == 127
        || (a == 100 && (64..=127).contains(&b))
        || (a == 172 && (16..=31).contains(&b))
        || (a == 192 && b == 168)
        || (a == 169 && b == 254)
        || (a == 192 && b == 0 && c == 2)
        || (a == 198 && (b == 18 || b == 19))
        || a >= 224
}

/// 纯判定逻辑(可单测):依据系统/公共 DNS 解析结果给出结论
pub fn classify_dns_consistency(
    system_ips: &[String],
    public_ips: &[String],
) -> (String, Option<String>) {
    if system_ips.is_empty() && public_ips.is_empty() {
        return ("未知".into(), Some("所有 DNS 服务器均无响应".into()));
    }
    let system_bogon = system_ips.iter().any(|ip| {
        ip.parse::<std::net::Ipv4Addr>()
            .map(is_bogon_ipv4)
            .unwrap_or(false)
    });
    if system_bogon {
        return (
            "可疑劫持".into(),
            Some("系统 DNS 返回了保留/内网地址,存在 DNS 劫持或强制门户可能".into()),
        );
    }
    if system_ips.is_empty() {
        return (
            "异常".into(),
            Some("系统 DNS 无响应而公共 DNS 正常,建议检查路由器/运营商 DNS 设置".into()),
        );
    }
    if public_ips.is_empty() {
        return ("未知".into(), Some("公共 DNS 均无响应,无法交叉验证".into()));
    }
    let sys: std::collections::HashSet<&String> = system_ips.iter().collect();
    let public: std::collections::HashSet<&String> = public_ips.iter().collect();
    if sys.is_subset(&public) {
        ("一致".into(), None)
    } else {
        (
            "不一致".into(),
            Some("系统 DNS 结果与公共 DNS 不同:可能为 CDN 地域调度,也可能被运营商改写".into()),
        )
    }
}

pub async fn dns_consistency(domain: &str) -> DnsConsistencyReport {
    let domain = domain.trim().to_string();
    let mut entries: Vec<DnsConsistencyEntry> = Vec::new();

    let ifaces = collect_interfaces();
    let system: Vec<String> = ifaces
        .iter()
        .find(|i| i.is_default)
        .map(|i| i.dns_servers.clone())
        .unwrap_or_default();
    let publics = ["223.5.5.5", "119.29.29.29", "8.8.8.8", "1.1.1.1"];

    for s in system.iter().take(3) {
        if let Ok(ip) = s.parse::<IpAddr>() {
            let p = dns_udp_probe(ip, &domain).await;
            entries.push(DnsConsistencyEntry {
                server: s.clone(),
                source: "系统".into(),
                ok: p.ok && p.resolved_ip.is_some(),
                rtt_ms: p.rtt_ms,
                ips: p.resolved_ip.iter().cloned().collect(),
                error: p.error,
            });
        }
    }
    for s in publics {
        if let Ok(ip) = s.parse::<IpAddr>() {
            let p = dns_udp_probe(ip, &domain).await;
            entries.push(DnsConsistencyEntry {
                server: s.to_string(),
                source: "公共".into(),
                ok: p.ok && p.resolved_ip.is_some(),
                rtt_ms: p.rtt_ms,
                ips: p.resolved_ip.iter().cloned().collect(),
                error: p.error,
            });
        }
    }

    let system_ips: Vec<String> = entries
        .iter()
        .filter(|e| e.source == "系统" && e.ok)
        .flat_map(|e| e.ips.clone())
        .collect();
    let public_ips: Vec<String> = entries
        .iter()
        .filter(|e| e.source == "公共" && e.ok)
        .flat_map(|e| e.ips.clone())
        .collect();
    let (verdict, note) = classify_dns_consistency(&system_ips, &public_ips);

    DnsConsistencyReport {
        domain,
        entries,
        verdict,
        note,
    }
}

// ============================================================
// NAT 类型检测(RFC 5389 STUN,纯 UDP 零依赖,简化判定)
// ============================================================

#[derive(Debug, Clone, Serialize)]
pub struct NatServerEntry {
    pub server: String,
    pub ok: bool,
    pub mapped_ip: Option<String>,
    pub mapped_port: Option<u16>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct NatReport {
    /// 公网直连 | 锥形 NAT | 对称 NAT | 受限信息 | 未知
    pub nat_type: String,
    pub local_ip: Option<String>,
    pub servers: Vec<NatServerEntry>,
    pub note: Option<String>,
}

const STUN_SERVERS: &[(&str, u16)] = &[("stun.qq.com", 3478), ("stun.miwifi.com", 3478)];

/// 构造 STUN Binding Request(RFC 5389)
pub fn build_stun_request(txid: [u8; 12]) -> Vec<u8> {
    let mut p = Vec::with_capacity(20);
    p.extend_from_slice(&0x0001u16.to_be_bytes());
    p.extend_from_slice(&0u16.to_be_bytes());
    p.extend_from_slice(&0x2112A442u32.to_be_bytes());
    p.extend_from_slice(&txid);
    p
}

#[derive(Debug, Clone, PartialEq)]
pub struct MappedAddress {
    pub ip: std::net::Ipv4Addr,
    pub port: u16,
}

/// 解析 Binding Response 的 XOR-MAPPED-ADDRESS(0x0020)或 MAPPED-ADDRESS(0x0001)
pub fn parse_stun_response(buf: &[u8], txid: &[u8; 12]) -> Option<MappedAddress> {
    if buf.len() < 20 {
        return None;
    }
    let msg_type = u16::from_be_bytes([buf[0], buf[1]]);
    if msg_type != 0x0101 {
        return None; // 仅接受 Binding Success Response
    }
    if &buf[8..20] != txid.as_slice() {
        return None; // 事务 ID 不匹配
    }
    let len = u16::from_be_bytes([buf[2], buf[3]]) as usize;
    let end = (20 + len).min(buf.len());
    let mut pos = 20usize;
    while pos + 4 <= end {
        let attr = u16::from_be_bytes([buf[pos], buf[pos + 1]]);
        let alen = u16::from_be_bytes([buf[pos + 2], buf[pos + 3]]) as usize;
        let vstart = pos + 4;
        if vstart + alen > end {
            break;
        }
        if alen >= 8 && (attr == 0x0020 || attr == 0x0001) {
            let port_raw = u16::from_be_bytes([buf[vstart + 2], buf[vstart + 3]]);
            let (ip, port) = if attr == 0x0020 {
                let xored = u32::from_be_bytes([
                    buf[vstart + 4],
                    buf[vstart + 5],
                    buf[vstart + 6],
                    buf[vstart + 7],
                ]);
                (
                    std::net::Ipv4Addr::from(xored ^ 0x2112A442),
                    port_raw ^ 0x2112,
                )
            } else {
                (
                    std::net::Ipv4Addr::new(
                        buf[vstart + 4],
                        buf[vstart + 5],
                        buf[vstart + 6],
                        buf[vstart + 7],
                    ),
                    port_raw,
                )
            };
            return Some(MappedAddress { ip, port });
        }
        pos = vstart + ((alen + 3) & !3); // TLV 4 字节对齐
    }
    None
}

/// 向单个 STUN 服务器发起 Binding Request
async fn stun_probe(host: &str, port: u16) -> NatServerEntry {
    let base = NatServerEntry {
        server: format!("{host}:{port}"),
        ok: false,
        mapped_ip: None,
        mapped_port: None,
        error: None,
    };
    use rand::Rng;
    let mut txid = [0u8; 12];
    rand::rng().fill_bytes(&mut txid);
    let packet = build_stun_request(txid);

    let res = tokio::time::timeout(Duration::from_millis(3500), async move {
        use tokio::net::UdpSocket;
        let mut addrs = tokio::net::lookup_host((host, port))
            .await
            .map_err(|e| e.to_string())?;
        let addr = addrs.find(|a| a.is_ipv4()).ok_or("无 IPv4 解析记录")?;
        let sock = UdpSocket::bind(if addr.is_ipv4() {
            "0.0.0.0:0"
        } else {
            "[::]:0"
        })
        .await
        .map_err(|e| e.to_string())?;
        sock.connect(addr).await.map_err(|e| e.to_string())?;
        sock.send(&packet).await.map_err(|e| e.to_string())?;
        let mut buf = [0u8; 256];
        // 最多收 3 包应对偶发乱序
        for _ in 0..3 {
            let n = sock.recv(&mut buf).await.map_err(|e| e.to_string())?;
            if let Some(m) = parse_stun_response(&buf[..n], &txid) {
                return Ok::<MappedAddress, String>(m);
            }
        }
        Err("响应无法解析".to_string())
    })
    .await;

    match res {
        Err(_) => NatServerEntry {
            error: Some("查询超时(3.5s)".into()),
            ..base
        },
        Ok(Err(e)) => NatServerEntry {
            error: Some(e),
            ..base
        },
        Ok(Ok(m)) => NatServerEntry {
            ok: true,
            mapped_ip: Some(m.ip.to_string()),
            mapped_port: Some(m.port),
            error: None,
            ..base
        },
    }
}

/// NAT 类型检测(简化判定:两次外部映射对比)
pub async fn nat_type() -> NatReport {
    let local = collect_interfaces()
        .iter()
        .find(|i| i.is_default)
        .and_then(|i| i.ipv4.first().cloned())
        .and_then(|s| {
            s.split('/')
                .next()
                .and_then(|x| x.parse::<std::net::Ipv4Addr>().ok())
        });

    let mut servers = Vec::new();
    for (h, p) in STUN_SERVERS {
        servers.push(stun_probe(h, *p).await);
    }

    let ok: Vec<&NatServerEntry> = servers
        .iter()
        .filter(|e| e.ok && e.mapped_ip.is_some())
        .collect();
    let (nat_type, note) = match ok.len() {
        0 => (
            "未知".to_string(),
            Some("无法连接任何 STUN 服务器(UDP 3478 可能被封锁)".to_string()),
        ),
        1 => (
            "受限信息".to_string(),
            Some("仅一个 STUN 服务器可达,判定至少需要两个".to_string()),
        ),
        _ => {
            let a = ok[0];
            let b = ok[1];
            let a_ip = a.mapped_ip.clone().unwrap_or_default();
            let b_ip = b.mapped_ip.clone().unwrap_or_default();
            let a_port = a.mapped_port.unwrap_or(0);
            let b_port = b.mapped_port.unwrap_or(0);
            if local.map(|l| l.to_string()) == Some(a_ip.clone()) && local.is_some() {
                (
                    "公网直连".to_string(),
                    Some("外部映射地址与本机地址一致,未经过 NAT".to_string()),
                )
            } else if a_ip == b_ip && a_port == b_port {
                (
                    "锥形 NAT".to_string(),
                    Some("不同 STUN 服务器映射一致(简化判定),通常对游戏/语音 P2P 友好".to_string()),
                )
            } else {
                (
                    "对称 NAT".to_string(),
                    Some("不同目标的映射端口不同(简化判定),P2P 直连可能受限".to_string()),
                )
            }
        }
    };
    NatReport {
        nat_type,
        local_ip: local.map(|i| i.to_string()),
        servers,
        note,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_dns_query_bytes() {
        let q = build_dns_query("www.example.com", 0x1234).expect("合法域名应构造成功");
        assert_eq!(&q[0..2], &[0x12, 0x34]); // ID
        assert_eq!(&q[2..4], &[0x01, 0x00]); // RD
        assert_eq!(&q[4..6], &[0, 1]); // QDCOUNT
                                       // QNAME: 3www7example3com0
        assert_eq!(q[12], 3);
        assert_eq!(&q[13..16], b"www");
        // 结尾 4 字节: QTYPE=1, QCLASS=1
        assert_eq!(&q[q.len() - 4..], &[0, 1, 0, 1]);
    }

    #[test]
    fn test_dns_query_rejects_oversized_labels() {
        // 单标签 >63 字节:拒绝(不得 as u8 截断出畸形长度)
        let long_label = "a".repeat(64);
        assert!(build_dns_query(&format!("{long_label}.com"), 1).is_none());
        // 63 字节边界:允许
        assert!(build_dns_query(&format!("{}.com", "a".repeat(63)), 1).is_some());
        // 总名长 >255 字节:拒绝(5 段 ×60 + 分隔 = 300+)
        let many = (0..5).map(|_| "b".repeat(60)).collect::<Vec<_>>().join(".");
        assert!(build_dns_query(&many, 1).is_none());
    }

    #[test]
    fn test_dns_reply_parse() {
        // 构造一个最小响应:头 + 问题 + 一条 A 记录
        let mut buf = Vec::new();
        buf.extend_from_slice(&0x1234u16.to_be_bytes());
        buf.extend_from_slice(&0x8180u16.to_be_bytes());
        buf.extend_from_slice(&1u16.to_be_bytes()); // qd
        buf.extend_from_slice(&1u16.to_be_bytes()); // an
        buf.extend_from_slice(&[0; 4]);
        // question: 1a0 = "\x01a\x00"
        buf.extend_from_slice(&[1, b'a', 0, 0, 1, 0, 1]);
        // answer: name pointer 0xC00C, type A, class IN, ttl, rdlen 4, rdata 1.2.3.4
        buf.extend_from_slice(&[0xC0, 0x0C, 0, 1, 0, 1, 0, 0, 0, 60, 0, 4, 1, 2, 3, 4]);
        let (id, an, a) = parse_dns_reply(&buf).unwrap();
        assert_eq!(id, 0x1234);
        assert_eq!(an, 1);
        assert_eq!(a, Some(std::net::Ipv4Addr::new(1, 2, 3, 4)));
    }

    #[test]
    fn test_dns_reply_parse_truncated() {
        assert!(parse_dns_reply(&[0, 1, 2]).is_none());
    }

    #[test]
    fn test_valid_host() {
        assert!(valid_host("speed.cloudflare.com"));
        assert!(valid_host("223.5.5.5"));
        assert!(valid_host("2606:4700:4700::1111"));
        assert!(!valid_host("bad host; rm -rf"));
        assert!(!valid_host(""));
        assert!(!valid_host("a\tb"));
    }

    #[test]
    fn test_parse_wifi_details_fields() {
        let text = "\
无线电类型    : IEEE 802.11n\r\n\
SSID          : MyHomeNet\r\n\
BSSID         : 84:16:f9:aa:bb:cc\r\n\
信号          : 92%\r\n\
信道          : 11\r\n\
接收速率(Mbps): 866.7\r\n\
传输速率(Mbps): 400.0\r\n";
        let d = parse_wifi_details(text).expect("应解析成功");
        assert_eq!(d.ssid.as_deref(), Some("MyHomeNet"));
        assert_eq!(
            d.bssid.as_deref(),
            Some("84:16:f9:aa:bb:cc"),
            "BSSID 必须能解析"
        );
        assert_eq!(d.signal_percent, Some(92));
        assert_eq!(d.radio_type.as_deref(), Some("IEEE 802.11n"));
        assert_eq!(d.channel.as_deref(), Some("11"));
        assert_eq!(d.receive_rate_mbps, Some(866.7));
        assert_eq!(d.transmit_rate_mbps, Some(400.0));
    }

    #[test]
    fn test_parse_wifi_details_absent_returns_none() {
        assert!(parse_wifi_details(
            "以太网适配器 以太网:\r\n  物理地址. . . : 00-11-22-33-44-55\r\n"
        )
        .is_none());
    }

    /// 判定是否为私有/链路本地地址(网关断言用,兼容 10.x / 172.16-31.x / IPv6)
    fn is_private_or_link_local(ip: &str) -> bool {
        if let Ok(v4) = ip.parse::<std::net::Ipv4Addr>() {
            let o = v4.octets();
            return o[0] == 10
                || o[0] == 127
                || (o[0] == 172 && (16..=31).contains(&o[1]))
                || (o[0] == 192 && o[1] == 168)
                || (o[0] == 169 && o[1] == 254);
        }
        let ip = ip.to_lowercase();
        ip == "::1" || ip.starts_with("fe80") || ip.starts_with("fc") || ip.starts_with("fd")
    }

    /// IcmpSendEcho 真实回环测试(无需管理员权限与网络)
    #[cfg(windows)]
    #[test]
    fn test_icmp_api_loopback() {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        let out = rt
            .block_on(icmp_ping_api("127.0.0.1", 3, 1000))
            .expect("IcmpSendEcho 回环应成功");
        assert_eq!(out.loss_percent, Some(0.0));
        assert_eq!(out.rtts_ms.len(), 3);
        assert!(out.avg_ms.is_some());
    }

    /// 无效主机:API 解析失败 → 返回 None(由子进程兜底,那里也失败 → loss None)
    #[cfg(windows)]
    #[test]
    fn test_icmp_api_bad_host_falls_back() {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        let out = rt.block_on(icmp_ping_api("this-host-does-not-exist.invalid", 2, 500));
        assert!(out.is_none());
    }

    #[test]
    fn test_bogon_detection() {
        use std::net::Ipv4Addr;
        assert!(is_bogon_ipv4(Ipv4Addr::new(192, 168, 1, 1)));
        assert!(is_bogon_ipv4(Ipv4Addr::new(10, 0, 0, 5)));
        assert!(is_bogon_ipv4(Ipv4Addr::new(127, 0, 0, 1)));
        assert!(is_bogon_ipv4(Ipv4Addr::new(100, 100, 1, 1))); // CGN
        assert!(is_bogon_ipv4(Ipv4Addr::new(172, 20, 0, 1)));
        assert!(is_bogon_ipv4(Ipv4Addr::new(169, 254, 3, 4)));
        assert!(is_bogon_ipv4(Ipv4Addr::new(224, 0, 0, 1))); // 组播
        assert!(is_bogon_ipv4(Ipv4Addr::new(0, 1, 2, 3)));
        assert!(!is_bogon_ipv4(Ipv4Addr::new(223, 5, 5, 5)));
        assert!(!is_bogon_ipv4(Ipv4Addr::new(8, 8, 8, 8)));
        assert!(!is_bogon_ipv4(Ipv4Addr::new(104, 16, 124, 96)));
        assert!(!is_bogon_ipv4(Ipv4Addr::new(100, 200, 1, 1))); // CGN 范围之外
    }

    #[test]
    fn test_classify_dns() {
        let sys = vec!["104.16.124.96".to_string()];
        let public = vec!["104.16.124.96".to_string(), "104.16.123.96".to_string()];
        let (v, _) = classify_dns_consistency(&sys, &public);
        assert_eq!(v, "一致");

        let (v, n) = classify_dns_consistency(&["192.168.1.1".to_string()], &public);
        assert_eq!(v, "可疑劫持");
        assert!(n.unwrap().contains("保留"));

        let (v, _) = classify_dns_consistency(&["1.2.3.4".to_string()], &public);
        assert_eq!(v, "不一致");

        let (v, _) = classify_dns_consistency(&[], &[]);
        assert_eq!(v, "未知");

        let (v, n) = classify_dns_consistency(&[], &public);
        assert_eq!(v, "异常");
        assert!(n.unwrap().contains("路由器"));

        let (v, _) = classify_dns_consistency(&sys, &[]);
        assert_eq!(v, "未知");
    }

    #[test]
    fn test_stun_roundtrip() {
        let txid = [7u8; 12];
        let req = build_stun_request(txid);
        assert_eq!(&req[0..2], &[0x00, 0x01]); // Binding Request
        assert_eq!(&req[4..8], &[0x21, 0x12, 0xA4, 0x42]); // magic cookie
        assert_eq!(&req[8..20], &txid);

        // 构造 Binding Success Response:XOR-MAPPED-ADDRESS 1.2.3.4:56789
        let ip = std::net::Ipv4Addr::new(1, 2, 3, 4);
        let port: u16 = 56789;
        let xor_ip = u32::from(ip) ^ 0x2112A442;
        let xor_port = port ^ 0x2112;
        let mut buf = Vec::new();
        buf.extend_from_slice(&0x0101u16.to_be_bytes());
        buf.extend_from_slice(&12u16.to_be_bytes()); // attr 总长 4+8
        buf.extend_from_slice(&0x2112A442u32.to_be_bytes());
        buf.extend_from_slice(&txid);
        buf.extend_from_slice(&0x0020u16.to_be_bytes());
        buf.extend_from_slice(&8u16.to_be_bytes());
        buf.extend_from_slice(&0x0001u16.to_be_bytes()); // family IPv4
        buf.extend_from_slice(&xor_port.to_be_bytes());
        buf.extend_from_slice(&xor_ip.to_be_bytes());

        let m = parse_stun_response(&buf, &txid).expect("应解析成功");
        assert_eq!(m.ip, ip);
        assert_eq!(m.port, port);
    }

    #[test]
    fn test_stun_malformed() {
        let txid = [9u8; 12];
        assert!(parse_stun_response(&[0, 1, 2], &txid).is_none()); // 截断
                                                                   // 类型错误(0x0111 = Binding Error Response)
        let mut buf = build_stun_request(txid);
        buf[0..2].copy_from_slice(&0x0111u16.to_be_bytes());
        assert!(parse_stun_response(&buf, &txid).is_none());
        // 事务 ID 不匹配
        let other = [1u8; 12];
        let mut buf2 = build_stun_request(txid);
        buf2[8..20].copy_from_slice(&other);
        assert!(parse_stun_response(&buf2, &txid).is_none());
    }

    /// 真实网络:MTR 式路由质量(对公共 DNS,少跳数快验证)
    #[test]
    #[ignore = "需要真实网络,离线/CI 下运行会失败"]
    fn test_route_quality_real_network() {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        let report = rt
            .block_on(route_quality("223.5.5.5", 8, 3))
            .expect("路由质量分析应成功");
        assert!(!report.hops.is_empty(), "至少应解析出一跳");
        // 首跳应为本地网关(可达);网段依环境而异(10.x/172.16-31.x/192.168.x/IPv6 ULA 等)
        let first = &report.hops[0];
        assert_eq!(first.hop, 1);
        if let Some(ip) = &first.ip {
            assert!(
                is_private_or_link_local(ip),
                "首跳应为私有网关地址,实际 {ip}"
            );
        }
        // 存在至少一跳有真实样本
        assert!(
            report.hops.iter().any(|h| h.samples > 0),
            "至少一跳应有 ICMP 样本"
        );
    }

    /// 真实网络:STUN NAT 检测(至少尽力执行,不强制成功——受网络策略影响)
    #[test]
    #[ignore = "需要真实网络,离线/CI 下运行会失败"]
    fn test_nat_type_real_network() {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        let report = rt.block_on(nat_type());
        assert!(!report.nat_type.is_empty());
        // 每个服务器条目要么成功带映射,要么带错误说明
        for s in &report.servers {
            assert!(s.mapped_ip.is_some() || s.error.is_some());
        }
    }
}

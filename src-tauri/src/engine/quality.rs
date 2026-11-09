//! 网络质量分析:抖动计算、ICMP 输出解析(语言无关)、可解释的质量评级。
//!
//! 关键原则:
//! - HTTP 请求失败 ≠ 网络丢包。丢包率只来自 ICMP 实测;HTTP 失败单独计数。
//! - 评级必须结合测试条件(负载延迟比、抖动、丢包),并动态生成说明文字。

use std::sync::LazyLock;

use regex::Regex;

use super::model::{QualityAssessment, TestResult};

// 解析路径(诊断/拨测每秒执行)复用编译结果,不在每次调用时重新编译
static LOSS_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(\d+(?:\.\d+)?)\s*%").unwrap());
// 单次回复格式:"时间=25ms"/"time<1ms"/"time=25ms"(符号后紧跟数字);
// 统计行("最短 = 15ms"/"Minimum = 0ms")符号后带空格,据此排除。
static PING_TIME_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"[<=](\d+(?:\.\d+)?)\s*ms").unwrap());
static AVG_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?:平均|Average)\s*=\s*(\d+(?:\.\d+)?)\s*ms").unwrap());
static IPV4_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\b(\d{1,3}\.\d{1,3}\.\d{1,3}\.\d{1,3})\b").unwrap());
static IPV6_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"\b(?:[0-9A-Fa-f]{1,4}:){2,7}[0-9A-Fa-f:]{1,4}\b").unwrap()
});
static TRACE_TIME_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(<)?\s*(\d+)\s*ms").unwrap());

/// 计算延迟抖动(RFC 3550 风格:相邻样本差的绝对值均值)
pub fn jitter_ms(samples: &[f64]) -> f64 {
    if samples.len() < 2 {
        return 0.0;
    }
    let mut acc = 0.0;
    for w in samples.windows(2) {
        acc += (w[1] - w[0]).abs();
    }
    acc / (samples.len() - 1) as f64
}

/// 中位数
pub fn median(values: &mut Vec<f64>) -> f64 {
    if values.is_empty() {
        return 0.0;
    }
    values.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let n = values.len();
    if n % 2 == 1 {
        values[n / 2]
    } else {
        (values[n / 2 - 1] + values[n / 2]) / 2.0
    }
}

/// 解析系统 ping 输出(中文/英文 Windows 通用,语言无关正则):
/// - 丢包率: 匹配 "(0% 丢失)" / "(0% loss)"
/// - RTT:    匹配 "时间=25ms" / "time<1ms" / "time=25ms"
pub struct PingOutput {
    pub loss_percent: Option<f64>,
    pub rtts_ms: Vec<f64>,
    pub avg_ms: Option<f64>,
}

pub fn parse_ping_output(stdout: &str) -> PingOutput {
    let loss_re = &*LOSS_RE;
    let time_re = &*PING_TIME_RE;
    let avg_re = &*AVG_RE;

    let loss_percent = loss_re
        .captures(stdout)
        .and_then(|c| c[1].parse::<f64>().ok());
    let rtts_ms: Vec<f64> = time_re
        .captures_iter(stdout)
        .filter_map(|c| c[1].parse::<f64>().ok())
        .collect();
    // 平均值优先采用统计行,否则由 RTT 样本计算
    let avg_ms = avg_re
        .captures(stdout)
        .and_then(|c| c[1].parse::<f64>().ok())
        .or_else(|| {
            if rtts_ms.is_empty() {
                None
            } else {
                Some(rtts_ms.iter().sum::<f64>() / rtts_ms.len() as f64)
            }
        });
    PingOutput {
        loss_percent,
        rtts_ms,
        avg_ms,
    }
}

/// 解析 tracert 输出为一跳列表(语言无关):
/// - IP:IPv4 / IPv6 正则提取
/// - 无 IP 且含 `*` → 超时跳
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct TraceHop {
    pub hop: u32,
    pub ip: Option<String>,
    pub times_ms: Vec<f64>,
    pub timeout: bool,
}

pub fn parse_traceroute_output(stdout: &str) -> Vec<TraceHop> {
    let ipv4_re = &*IPV4_RE;
    let ipv6_re = &*IPV6_RE;
    let time_re = &*TRACE_TIME_RE;
    let mut hops: Vec<TraceHop> = Vec::new();
    for line in stdout.lines() {
        // 仅处理以跳数开头的行
        let trimmed = line.trim_start();
        let Some((num_str, rest)) = trimmed.split_once(|c: char| !c.is_ascii_digit()) else {
            continue;
        };
        let Ok(hop_num) = num_str.parse::<u32>() else {
            continue;
        };
        if hop_num == 0 || hop_num > 64 || rest.is_empty() {
            continue;
        }
        let ip = ipv4_re
            .captures(rest)
            .map(|c| c[1].to_string())
            .or_else(|| ipv6_re.captures(rest).map(|c| c[0].to_string()));
        let times_ms: Vec<f64> = time_re
            .captures_iter(rest)
            .map(|c| c[2].parse::<f64>().unwrap_or(0.0))
            .collect();
        let timeout = ip.is_none() && (rest.contains('*') || times_ms.is_empty());
        if ip.is_none() && !timeout {
            continue;
        }
        // 同一跳可能跨行(先超时后到达):覆盖同一编号
        if let Some(existing) = hops.iter_mut().find(|h| h.hop == hop_num) {
            if existing.ip.is_none() && ip.is_some() {
                existing.ip = ip;
                existing.times_ms = times_ms;
                existing.timeout = false;
            }
        } else {
            hops.push(TraceHop {
                hop: hop_num,
                ip,
                times_ms,
                timeout,
            });
        }
    }
    hops
}

/// 从 1s 滑动窗口样本计算峰值速度
pub fn peak_from_samples(samples: &[[f64; 2]]) -> f64 {
    // 样本 [t_ms, mbps],间隔 200ms;1s 窗口 = 连续 5 个样本均值
    if samples.is_empty() {
        return 0.0;
    }
    if samples.len() < 5 {
        return samples.iter().map(|s| s[1]).fold(0.0, f64::max);
    }
    let mut best = 0.0f64;
    for w in samples.windows(5) {
        let avg = w.iter().map(|s| s[1]).sum::<f64>() / 5.0;
        best = best.max(avg);
    }
    best
}

/// 计算稳定窗口(剔除预热)后的平均速度
/// `counters_at: [(t_ms, bytes)]` 为阶段内里程碑采样
pub fn stable_window_mbps(
    total_bytes_at_end: u64,
    end_ms: f64,
    warmup_ms: f64,
    bytes_at_warmup: u64,
) -> f64 {
    if end_ms <= warmup_ms {
        // 阶段过短,退化为全窗口
        if end_ms <= 0.0 {
            return 0.0;
        }
        return super::model::bytes_per_sec_to_mbps(total_bytes_at_end as f64 / (end_ms / 1000.0));
    }
    let bytes = total_bytes_at_end.saturating_sub(bytes_at_warmup) as f64;
    let secs = (end_ms - warmup_ms) / 1000.0;
    if secs <= 0.0 {
        return 0.0;
    }
    super::model::bytes_per_sec_to_mbps(bytes / secs)
}

/// 结果分析器:依据真实数据生成评级与说明
pub fn analyze_quality(r: &TestResult) -> QualityAssessment {
    let mut score: f64 = 100.0;
    let mut notes: Vec<String> = Vec::new();

    // 1. 空闲延迟
    let idle = r.idle_ping_ms;
    if idle > 0.0 {
        let penalty = if idle < 20.0 {
            0.0
        } else if idle < 40.0 {
            5.0
        } else if idle < 70.0 {
            12.0
        } else if idle < 120.0 {
            22.0
        } else {
            35.0
        };
        score -= penalty;
        if penalty >= 12.0 {
            notes.push(format!(
                "空闲延迟 {:.0} ms 偏高,网页浏览与视频通话可能感到明显等待。",
                idle
            ));
        } else if idle < 20.0 {
            notes.push(format!("空闲延迟 {:.0} ms,非常理想,适合实时应用。", idle));
        }
    }

    // 2. 抖动
    let jitter = r.jitter_ms;
    if jitter > 0.0 {
        let penalty = if jitter < 5.0 {
            0.0
        } else if jitter < 10.0 {
            6.0
        } else if jitter < 20.0 {
            14.0
        } else {
            25.0
        };
        score -= penalty;
        if penalty >= 14.0 {
            notes.push(format!(
                "延迟抖动 {:.1} ms 较大,即使平均延迟不高,也可能导致游戏与语音时卡时不卡。",
                jitter
            ));
        }
    }

    // 3. 负载延迟(bufferbloat):结合具体测试条件评估
    if idle > 0.0 {
        for (label, loaded) in [("下载", r.loaded_down_ms), ("上传", r.loaded_up_ms)] {
            if let Some(l) = loaded {
                if l > 0.0 {
                    let ratio = l / idle;
                    let penalty = if ratio < 1.2 {
                        0.0
                    } else if ratio < 1.75 {
                        10.0
                    } else if ratio < 2.5 {
                        20.0
                    } else {
                        32.0
                    };
                    score -= penalty;
                    if penalty >= 20.0 {
                        notes.push(format!(
                            "{}负载下延迟升至 {:.0} ms(空闲 {:.0} ms 的 {:.1} 倍),带宽满载时网页游戏/视频会议可能明显卡顿。",
                            label, l, idle, ratio
                        ));
                    } else if penalty >= 10.0 {
                        notes.push(format!(
                            "{}负载下延迟上升 {:.0}%({:.0} ms → {:.0} ms),满载使用时对实时性要求高的应用会有可感知影响。",
                            label,
                            (ratio - 1.0) * 100.0,
                            idle,
                            l
                        ));
                    }
                }
            } else {
                notes.push(format!(
                    "{}负载延迟未获得(该阶段可能被取消或失败),评级未计入此条件。",
                    label
                ));
            }
        }
    }

    // 4. 丢包(ICMP 实测;HTTP 失败不混入)
    match r.packet_loss_percent {
        Some(loss) => {
            let penalty = if loss <= 0.0 {
                0.0
            } else if loss < 1.0 {
                10.0
            } else if loss < 3.0 {
                20.0
            } else if loss < 7.0 {
                35.0
            } else {
                50.0
            };
            score -= penalty;
            if loss > 0.0 {
                notes.push(format!(
                    "ICMP 实测丢包率 {:.1}%,数据需要重传,会同时降低吞吐并增加延迟。",
                    loss
                ));
            } else {
                notes.push("ICMP 实测无丢包,链路可靠性良好。".into());
            }
        }
        None => {
            notes.push(
                "未获得 ICMP 丢包数据(目标可能禁 ping 或系统限制),丢包不计入本次评级;吞吐阶段的 HTTP 请求失败已单独统计,不与丢包混淆。".into(),
            );
        }
    }

    // 5. HTTP 请求失败(测试条件说明);非 HTTP 协议的传输错误不得计入此字段。
    if r.http_failures > 0 {
        score -= (r.http_failures as f64 * 5.0).min(15.0);
        notes.push(format!(
            "吞吐测试期间发生 {} 次 HTTP 请求失败/超时(可能是服务器限流或链路波动),已酌情降分。",
            r.http_failures
        ));
    }

    // 6. 结论
    if notes.is_empty() {
        notes.push("各项指标均在理想范围内,未发现明显问题。".into());
    }
    let score = score.clamp(0.0, 100.0) as u32;
    let rating = if score >= 85 {
        "优秀"
    } else if score >= 70 {
        "良好"
    } else if score >= 50 {
        "一般"
    } else {
        "较差"
    };
    QualityAssessment {
        score,
        rating: rating.into(),
        notes,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_jitter() {
        assert_eq!(jitter_ms(&[]), 0.0);
        assert_eq!(jitter_ms(&[10.0]), 0.0);
        // (|12-10| + |9-12|) / 2 = 2.5
        let j = jitter_ms(&[10.0, 12.0, 9.0]);
        assert!((j - 2.5).abs() < 1e-9);
    }

    #[test]
    fn test_parse_ping_chinese() {
        let out = "\
正在 Ping 223.5.5.5 具有 32 字节的数据:
来自 223.5.5.5 的回复: 字节=32 时间=15ms TTL=118
来自 223.5.5.5 的回复: 字节=32 时间=16ms TTL=118
请求超时。
来自 223.5.5.5 的回复: 字节=32 时间=17ms TTL=118

223.5.5.5 的 Ping 统计信息:
    数据包: 已发送 = 4,已接收 = 3,丢失 = 1 (25% 丢失),
往返行程的估计时间(以毫秒为单位):
    最短 = 15ms,最长 = 17ms,平均 = 16ms";
        let p = parse_ping_output(out);
        assert_eq!(p.loss_percent, Some(25.0));
        assert_eq!(p.rtts_ms, vec![15.0, 16.0, 17.0]);
        assert_eq!(p.avg_ms, Some(16.0));
    }

    #[test]
    fn test_parse_ping_english() {
        let out = "Reply from 1.1.1.1: bytes=32 time=12ms TTL=57\n\
                   Reply from 1.1.1.1: bytes=32 time<1ms TTL=57\n\
                   Ping statistics for 1.1.1.1:\n\
                   Packets: Sent = 2, Received = 2, Lost = 0 (0% loss)\n\
                   Minimum = 0ms, Maximum = 12ms, Average = 6ms";
        let p = parse_ping_output(out);
        assert_eq!(p.loss_percent, Some(0.0));
        assert_eq!(p.rtts_ms, vec![12.0, 1.0]);
        assert_eq!(p.avg_ms, Some(6.0));
    }

    #[test]
    fn test_parse_traceroute_chinese() {
        let out = "\
通过最多 30 个跃点跟踪
到 1.2.3.4 的路由:

  1     2 ms     1 ms     2 ms  192.168.1.1
  2     *        *        *     请求超时。
  3    12 ms    11 ms    13 ms  10.100.0.1
  4    45 ms    44 ms    46 ms  1.2.3.4

跟踪完成。";
        let hops = parse_traceroute_output(out);
        assert_eq!(hops.len(), 4);
        assert_eq!(hops[0].ip, Some("192.168.1.1".into()));
        assert!(hops[1].timeout);
        assert!(hops[1].ip.is_none());
        assert_eq!(hops[2].times_ms, vec![12.0, 11.0, 13.0]);
        assert_eq!(hops[3].ip, Some("1.2.3.4".into()));
    }

    #[test]
    fn test_parse_traceroute_english() {
        let out = "Tracing route to example.com [93.184.216.34]\n\
                   over a maximum of 30 hops:\n\n\
                     1     1 ms     1 ms     1 ms  192.168.0.1 \n\
                     2     9 ms     9 ms     8 ms  10.0.0.1 \n";
        let hops = parse_traceroute_output(out);
        assert_eq!(hops.len(), 2);
        assert_eq!(hops[1].ip, Some("10.0.0.1".into()));
    }

    #[test]
    fn test_peak_and_stable() {
        let samples: Vec<[f64; 2]> = (0..10)
            .map(|i| [(i * 200) as f64, 100.0 + i as f64])
            .collect();
        // 1s 窗口均值最大 = 最后 5 个 (105+106+107+108+109)/5 = 107
        assert!((peak_from_samples(&samples) - 107.0).abs() < 1e-9);

        // 预热 1.2s(1200ms),结束时 10s,预热时字节 12_000_000,结束 100_000_000
        let mbps = stable_window_mbps(100_000_000, 10_000.0, 1_200.0, 12_000_000);
        // (100M-12M)/8.8s = 10_000_000 B/s = 80 Mbps
        assert!((mbps - 80.0).abs() < 1e-6);
    }

    #[test]
    fn test_rating_paths() {
        let mut r = TestResult::default();
        r.idle_ping_ms = 15.0;
        r.jitter_ms = 2.0;
        r.loaded_down_ms = Some(16.0);
        r.loaded_up_ms = Some(17.0);
        r.packet_loss_percent = Some(0.0);
        let q = analyze_quality(&r);
        assert_eq!(q.rating, "优秀");
        assert!(q.score >= 85);

        r.idle_ping_ms = 150.0;
        r.jitter_ms = 30.0;
        r.loaded_down_ms = Some(600.0);
        r.loaded_up_ms = Some(500.0);
        r.packet_loss_percent = Some(4.0);
        r.http_failures = 3;
        let q2 = analyze_quality(&r);
        assert_eq!(q2.rating, "较差");
        assert!(q2.notes.len() >= 4);
        assert!(q2.notes.iter().any(|n| n.contains("丢包")));
        assert!(q2.notes.iter().any(|n| n.contains("HTTP")));
    }

    #[test]
    fn test_rating_loss_unknown() {
        let mut r = TestResult::default();
        r.idle_ping_ms = 25.0;
        r.jitter_ms = 3.0;
        r.packet_loss_percent = None; // 无法 ICMP
        let q = analyze_quality(&r);
        assert!(q.notes.iter().any(|n| n.contains("ICMP")));
        assert!(q.score >= 70); // 不因无法测量而崩溃性降分
    }
}

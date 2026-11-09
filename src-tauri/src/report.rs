//! 测速报告生成:单条结果 → 自包含 HTML(无外部资源,便于存档)。

use crate::engine::model::TestResult;

fn esc(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

fn fmt(v: f64) -> String {
    format!("{v:.2}")
}

pub fn render_html_report(r: &TestResult) -> String {
    let quality = r.quality.clone().unwrap_or_default();
    let mut samples_note = String::new();
    if !r.down_samples.is_empty() {
        samples_note.push_str(&format!(
            "<tr><td>下载速度样本</td><td>{} 个(200ms 粒度)</td></tr>",
            r.down_samples.len()
        ));
    }
    if !r.up_samples.is_empty() {
        samples_note.push_str(&format!(
            "<tr><td>上传速度样本</td><td>{} 个(200ms 粒度)</td></tr>",
            r.up_samples.len()
        ));
    }
    if !r.latency_samples.is_empty() {
        samples_note.push_str(&format!(
            "<tr><td>延迟样本</td><td>{} 个</td></tr>",
            r.latency_samples.len()
        ));
    }
    let conditions = if r.conditions.is_empty() {
        String::new()
    } else {
        format!(
            "<p class='cond'>测试条件:{}</p>",
            esc(&r.conditions.join(";"))
        )
    };
    let error_block = r
        .error
        .as_ref()
        .map(|e| format!("<p class='err'>错误信息:{}</p>", esc(e)))
        .unwrap_or_default();

    format!(
        r#"<!DOCTYPE html>
<html lang="zh-CN"><head><meta charset="utf-8">
<title>NetVision Pro 测速报告</title>
<style>
body {{ background:#0a0e18; color:#e8f1ff; font-family:"Microsoft YaHei UI","Segoe UI",sans-serif; max-width:860px; margin:40px auto; padding:0 24px; }}
h1 {{ font-size:26px; color:#7dd3fc; }} h1 small {{ color:#8ba3c7; font-size:14px; font-weight:400; }}
table {{ width:100%; border-collapse:collapse; margin:16px 0; }}
td {{ padding:9px 12px; border-bottom:1px solid rgba(125,211,252,.12); }}
td:first-child {{ color:#8ba3c7; width:220px; }}
.big {{ font-size:30px; color:#22d3ee; font-weight:700; }}
.rating {{ display:inline-block; padding:4px 14px; border-radius:99px; background:rgba(34,211,238,.14); color:#7dd3fc; }}
ul {{ line-height:1.9; color:#c7d7ef; }}
.cond {{ color:#8ba3c7; font-size:13px; }} .err {{ color:#f87171; }}
.foot {{ color:#5b6b85; font-size:12px; margin-top:28px; }}
</style></head><body>
<h1>NetVision Pro 测速报告 <small>#{id}</small></h1>
<table>
<tr><td>测试状态</td><td><span class="rating">{status}</span></td></tr>
<tr><td>测试时间</td><td>{started} → {finished}({dur} 秒)</td></tr>
<tr><td>下载速度</td><td class="big">{dl} Mbps <span style="font-size:13px;color:#8ba3c7">(峰值 {dlp} Mbps)</span></td></tr>
<tr><td>上传速度</td><td>{ul}</td></tr>
<tr><td>空闲延迟</td><td>{ping} ms(最小 {pmin} / 最大 {pmax})</td></tr>
<tr><td>下载负载延迟</td><td>{ldl}</td></tr>
<tr><td>上传负载延迟</td><td>{lul}</td></tr>
<tr><td>抖动</td><td>{jit} ms</td></tr>
<tr><td>丢包率(ICMP 实测)</td><td>{loss}</td></tr>
<tr><td>HTTP 请求失败次数</td><td>{fails} 次(与丢包分别统计)</td></tr>
<tr><td>测速节点</td><td>{server}</td></tr>
<tr><td>网络类型 / 接口</td><td>{nettype} / {iface}</td></tr>
<tr><td>公网 IP</td><td>{ip}</td></tr>
<tr><td>ISP</td><td>{isp}</td></tr>
{samples}
</table>
{conditions}{error_block}
<h3>网络质量评级:{rating_badge}(得分 {score}/100)</h3>
<ul>{notes}</ul>
<p class="foot">本报告由 NetVision Pro v{ver} 生成;数据来自真实网络测量。网卡协商速度与测速服务器性能可能影响结果,不代表理论带宽上限。</p>
</body></html>"#,
        id = esc(&r.id),
        status = esc(&r.status),
        started = esc(&r.started_at),
        finished = esc(&r.finished_at),
        dur = r.duration_ms / 1000,
        dl = fmt(r.download_mbps),
        dlp = fmt(r.download_peak_mbps),
        ul = r.upload_mbps.map_or_else(|| "未获得(上传测试未成功)".into(), |v| format!("{} Mbps", fmt(v))),
        ping = fmt(r.idle_ping_ms),
        pmin = fmt(r.ping_min_ms),
        pmax = fmt(r.ping_max_ms),
        ldl = r.loaded_down_ms.map_or_else(|| "未测量".into(), |v| format!("{} ms", fmt(v))),
        lul = r.loaded_up_ms.map_or_else(|| "未测量".into(), |v| format!("{} ms", fmt(v))),
        jit = fmt(r.jitter_ms),
        loss = r.packet_loss_percent.map_or_else(|| "未获得(目标可能禁 ping)".into(), |v| format!("{} %", fmt(v))),
        fails = r.http_failures,
        server = r.server.as_ref().map_or_else(|| "-".into(), |s| esc(&s.name)),
        nettype = esc(&r.network_type),
        iface = esc(&r.interface_name),
        ip = r.public_ip.as_ref().map_or_else(|| "未获取(未授权)".into(), |v| esc(v)),
        isp = r.isp.as_ref().map_or_else(|| "未知".into(), |v| esc(v)),
        samples = samples_note,
        rating_badge = esc(&quality.rating),
        score = quality.score,
        notes = quality
            .notes
            .iter()
            .map(|n| format!("<li>{}</li>", esc(n)))
            .collect::<Vec<_>>()
            .join(""),
        ver = env!("CARGO_PKG_VERSION"),
    )
}

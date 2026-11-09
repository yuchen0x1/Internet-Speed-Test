// 网络诊断中心:本地/公网信息 + 各项检测(成功/失败/警告/未知如实展示)
import React, { useEffect } from "react";
import { api } from "../api";
import { useApp } from "../store/app";
import { useDiag } from "../store/diag";
import { Icon, type IconName } from "../components/Icon";
import type { IfaceInfo, WifiDetails } from "../types";

type Status = "ok" | "err" | "warn" | "unknown" | "running";

function Dot({ s }: { s: Status }) {
  const cls = s === "running" ? "warn pulse" : s;
  return <span className={`status-dot ${cls}`} />;
}

function ActionCard(props: {
  icon: IconName; title: string; desc: string;
  running: boolean;
  onRun: () => void;
  disabled?: boolean;
  children?: React.ReactNode;
  status?: Status;
}) {
  return (
    <div className="card pad hoverable">
      <div className="row" style={{ alignItems: "flex-start" }}>
        <span className="icon-wrap diag-icon"><Icon name={props.icon} size={19} /></span>
        <div className="grow">
          <div className="row gap8">
            <b style={{ fontSize: 13.5 }}>{props.title}</b>
            {props.status && <Dot s={props.status} />}
          </div>
          <div className="faint" style={{ fontSize: 11.5, marginTop: 3 }}>{props.desc}</div>
        </div>
        <button className="btn sm" onClick={props.onRun} disabled={props.running || props.disabled}>
          {props.running ? <Icon name="refresh" size={13} className="spin" /> : <Icon name="play" size={13} />}
          {props.running ? "检测中" : "开始检测"}
        </button>
      </div>
      {props.children && <div className="mt12">{props.children}</div>}
    </div>
  );
}

function ResultRow(props: { label: string; s?: Status; value?: string }) {
  return (
    <div className="row diag-row">
      <span className="grow dim">{props.label}</span>
      {props.s && <Dot s={props.s} />}
      <span className="mono" style={{ fontSize: 12, textAlign: "right", maxWidth: "60%" }}>{props.value ?? "未知"}</span>
    </div>
  );
}

export function DiagnosticsPage() {
  const toast = useApp((s) => s.toast);
  const settings = useApp((s) => s.settings);
  // 检测结果存于 store:切页重挂载后保留(见 store/diag.ts)
  const {
    local, setLocal, wifi, setWifi, pub, setPub,
    dns, setDns, dnsRun, setDnsRun,
    gw, setGw, gwRun, setGwRun,
    tcp, setTcp, tcpRun, setTcpRun,
    https, setHttps, httpsRun, setHttpsRun,
    v6, setV6, v6Run, setV6Run,
    routeHost, setRouteHost, route, setRoute, routeRun, setRouteRun,
    dnsDomain, setDnsDomain, dnsCons, setDnsCons, dnsConsRun, setDnsConsRun,
    nat, setNat, natRun, setNatRun,
    grid, setGrid, gridRun, setGridRun,
    traceHost, setTraceHost, trace, setTrace, traceRun, setTraceRun,
    pingHost, setPingHost, ping, setPing, pingRun, setPingRun,
  } = useDiag();

  const loadBase = async () => {
    try {
      const [l, w, p] = await Promise.all([
        api.diagLocal(),
        api.diagWifi().catch(() => null),
        api.diagPublic().catch(() => null),
      ]);
      setLocal(l);
      setWifi(w);
      setPub(p);
    } catch (e) {
      toast("error", `本地网络信息获取失败:${String(e)}`);
    }
  };
  useEffect(() => {
    loadBase();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  const runDns = async () => {
    setDnsRun(true);
    try { setDns(await api.diagDns()); } catch (e) { toast("error", String(e)); } finally { setDnsRun(false); }
  };
  const runGw = async () => {
    setGwRun(true);
    try { setGw(await api.diagGateway()); } catch (e) { toast("error", String(e)); } finally { setGwRun(false); }
  };
  const runTcp = async () => {
    setTcpRun(true);
    try { setTcp(await api.diagTcp()); } catch (e) { toast("error", String(e)); } finally { setTcpRun(false); }
  };
  const runHttps = async () => {
    setHttpsRun(true);
    try { setHttps(await api.diagHttps()); } catch (e) { toast("error", String(e)); } finally { setHttpsRun(false); }
  };
  const runV6 = async () => {
    setV6Run(true);
    try { setV6(await api.diagIpv6()); } catch (e) { toast("error", String(e)); } finally { setV6Run(false); }
  };
  const runRoute = async () => {
    setRouteRun(true);
    setRoute(null);
    try { setRoute(await api.diagRouteQuality(routeHost.trim(), 12, 6)); }
    catch (e) { toast("error", String(e)); }
    finally { setRouteRun(false); }
  };
  const runDnsCons = async () => {
    setDnsConsRun(true);
    setDnsCons(null);
    try { setDnsCons(await api.diagDnsConsistency(dnsDomain.trim())); }
    catch (e) { toast("error", String(e)); }
    finally { setDnsConsRun(false); }
  };
  const runNat = async () => {
    setNatRun(true);
    setNat(null);
    try { setNat(await api.diagNatType()); }
    catch (e) { toast("error", String(e)); }
    finally { setNatRun(false); }
  };
  const runGrid = async () => {
    setGridRun(true);
    setGrid(null);
    try { setGrid(await api.diagConnectivityGrid()); }
    catch (e) { toast("error", String(e)); }
    finally { setGridRun(false); }
  };
  const runTrace = async () => {
    setTraceRun(true);
    setTrace(null);
    try { setTrace(await api.diagTraceroute(traceHost.trim(), 15)); }
    catch (e) { toast("error", String(e)); }
    finally { setTraceRun(false); }
  };
  const runPing = async () => {
    setPingRun(true);
    try { setPing(await api.diagPing(pingHost.trim(), 8)); }
    catch (e) { toast("error", String(e)); }
    finally { setPingRun(false); }
  };

  const defaultIface = local?.interfaces.find((i) => i.is_default);

  return (
    <div>
      <div className="page-head">
        <div>
          <h1 className="page-title">
            <span className="icon-wrap"><Icon name="shield" size={22} /></span>
            网络诊断
          </h1>
          <div className="page-sub">本地网络 · 公网信息 · 连接检测 · 路由追踪(无法检测的项目如实标注)</div>
        </div>
        <button className="btn" onClick={loadBase}><Icon name="refresh" size={14} /> 刷新信息</button>
      </div>

      <div className="diag-grid">
        {/* 本地网络 */}
        <div className="card pad">
          <div className="chart-title"><Icon name="ethernet" size={15} /> 本地网络</div>
          {defaultIface ? <IfaceView i={defaultIface} wifi={wifi} /> : <div className="empty">未检测到默认网络接口</div>}
          {local && local.interfaces.length > 1 && (
            <details className="mt12">
              <summary className="dim" style={{ cursor: "pointer", fontSize: 12.5 }}>全部接口({local.interfaces.length})</summary>
              <div className="mt8">
                {local.interfaces.filter((i) => !i.is_default).map((i) => (
                  <IfaceView key={i.name} i={i} wifi={null} compact />
                ))}
              </div>
            </details>
          )}
          <ResultRow label="主机名" value={local?.hostname || "未知"} />
          <ResultRow label="系统" value={local?.os || "未知"} />
        </div>

        {/* 公网网络 */}
        <div className="card pad">
          <div className="chart-title"><Icon name="globe" size={15} /> 公网网络</div>
          {!settings?.show_public_ip ? (
            <div className="empty" style={{ padding: "22px 12px" }}>
              <Icon name="eye" size={26} />
              <span>公网 IP 显示已在隐私设置中关闭</span>
              <span style={{ fontSize: 11.5 }}>开启后才会向测速服务查询 IP 与运营商信息</span>
              <button className="btn sm mt8" onClick={() => useApp.getState().updateSettings({ show_public_ip: true })}>开启显示</button>
            </div>
          ) : (
            <>
              <ResultRow label="公网 IPv4" s={pub?.ipv4 ? "ok" : pub === null ? "unknown" : "err"} value={pub?.ipv4 ?? "未获取"} />
              <ResultRow label="公网 IPv6" s={pub?.ipv6 ? "ok" : "unknown"} value={pub?.ipv6 ?? "未检测到"} />
              <ResultRow label="ISP 运营商" value={pub?.isp ?? "暂无数据(来自测速节点报告)"} />
              <ResultRow label="边缘节点 (PoP)" value={pub?.edge_colo ?? "未知"} />
              <ResultRow label="国家/地区" value={pub?.country ?? "未知"} />
            </>
          )}
        </div>
      </div>

      {/* 诊断操作 */}
      <div className="diag-actions mt16">
        <ActionCard icon="layers" title="DNS 解析检测" desc="系统解析常见域名 + 对每个配置的 DNS 服务器直接 UDP 查询" running={dnsRun} onRun={runDns} status={dns ? (dns.domains.every((d) => d.ok) ? "ok" : dns.domains.some((d) => d.ok) ? "warn" : "err") : undefined}>
          {dns && (
            <>
              {dns.domains.filter((d) => d.domain !== "__hint__").map((d) => (
                <ResultRow key={d.domain} label={d.domain} s={d.ok ? "ok" : "err"} value={`${d.ms != null ? d.ms.toFixed(1) + " ms" : (d.error ?? "失败")}${d.ips.length ? ` · ${d.ips[0]}` : ""}`} />
              ))}
              {dns.domains.filter((d) => d.domain === "__hint__").map((d) => (
                <div key="hint" className="chip amber mt8" style={{ whiteSpace: "normal", lineHeight: 1.5 }}>{d.error}</div>
              ))}
              {dns.server_probes.map((p) => (
                <ResultRow key={p.server} label={`DNS 服务器 ${p.server}`} s={p.ok && p.answer_count > 0 ? "ok" : p.ok ? "warn" : "err"} value={p.ok ? `${p.rtt_ms?.toFixed(1)} ms · ${p.answer_count} 条记录${p.resolved_ip ? ` · ${p.resolved_ip}` : ""}` : (p.error ?? "失败")} />
              ))}
            </>
          )}
        </ActionCard>

        <ActionCard icon="wifi" title="默认网关连通性" desc="对默认网关执行 ICMP 检测(真实丢包与延迟)" running={gwRun} onRun={runGw} status={gw ? (gw.reachable ? "ok" : gw.loss_percent == null ? "unknown" : "err") : undefined}>
          {gw && (
            <>
              <ResultRow label="网关地址" value={gw.gateway ?? "未找到"} />
              <ResultRow label="连通性" s={gw.reachable ? "ok" : gw.loss_percent == null ? "unknown" : "err"} value={gw.reachable ? `平均 ${gw.avg_ms?.toFixed(1)} ms` : "不可达"} />
              <ResultRow label="丢包率" value={gw.loss_percent != null ? `${gw.loss_percent}%` : "未知"} />
              {gw.note && <div className="faint mt8" style={{ fontSize: 11.5 }}>{gw.note}</div>}
            </>
          )}
        </ActionCard>

        <ActionCard icon="zap" title="TCP 连接检测" desc="对常用服务端口发起真实 TCP 连接并计时" running={tcpRun} onRun={runTcp} status={tcp ? (tcp.every((r) => r.ok) ? "ok" : tcp.some((r) => r.ok) ? "warn" : "err") : undefined}>
          {tcp?.map((r) => (
            <ResultRow key={`${r.target}:${r.port}`} label={`${r.target}:${r.port}`} s={r.ok ? "ok" : "err"} value={r.ok ? `${r.ms?.toFixed(1)} ms` : (r.error ?? "失败")} />
          ))}
        </ActionCard>

        <ActionCard icon="shield" title="HTTPS 连接检测" desc="真实 TLS 请求验证 HTTPS 可达性与响应时间" running={httpsRun} onRun={runHttps} status={https ? (https.every((r) => r.ok) ? "ok" : https.some((r) => r.ok) ? "warn" : "err") : undefined}>
          {https?.map((r) => (
            <ResultRow key={r.url} label={r.url.replace("https://", "").slice(0, 34)} s={r.ok ? "ok" : "err"} value={r.ok ? `HTTP ${r.status} · ${r.ms?.toFixed(0)} ms` : (r.error ?? "失败")} />
          ))}
        </ActionCard>

        <ActionCard icon="globe" title="IPv6 可用性" desc="尝试通过纯 IPv6 端点建立连接" running={v6Run} onRun={runV6} status={v6 ? (v6.available ? "ok" : "err") : undefined}>
          {v6 && (
            <ResultRow label="IPv6 连接" s={v6.available ? "ok" : "err"} value={v6.available ? (v6.public_address ? `可用 · ${v6.public_address}` : "可用") : (v6.error ?? "不可用")} />
          )}
        </ActionCard>

        <ActionCard icon="trending" title="路由质量分析(MTR 式)" desc="tracert 获取路径后对每跳真实 ICMP 探测,定位劣化在接入段还是骨干段" running={routeRun} onRun={runRoute}>
          <div className="row mb8">
            <input className="text-input grow" value={routeHost} onChange={(e) => setRouteHost(e.target.value)} placeholder="目标主机" />
          </div>
          {route && route.hops.length > 0 && (
            <div className="table-wrap" style={{ maxHeight: 260, overflowY: "auto" }}>
              <table className="table">
                <thead><tr><th>跳</th><th>IP</th><th>丢包</th><th>平均</th><th>抖动</th><th>样本</th></tr></thead>
                <tbody>
                  {route.hops.map((h) => (
                    <tr key={h.hop}>
                      <td className="num">{h.hop}</td>
                      <td className="num">{h.ip ?? "*"}</td>
                      <td className="num">{h.loss_percent != null ? `${h.loss_percent.toFixed(0)}%` : "—"}</td>
                      <td className="num">{h.avg_ms != null ? `${h.avg_ms.toFixed(1)} ms` : "—"}</td>
                      <td className="num">{h.jitter_ms != null ? `${h.jitter_ms.toFixed(1)} ms` : "—"}</td>
                      <td>{h.timeout ? <span className="chip amber">超时</span> : <span className={`chip ${h.samples ? "green" : "red"}`}>{h.samples ? `${h.samples} 包` : "无响应"}</span>}</td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
          )}
          {route && route.hops.length === 0 && <div className="faint mt8" style={{ fontSize: 11.5 }}>未能解析出路由跳点。</div>}
        </ActionCard>

        <ActionCard icon="search" title="DNS 一致性 / 劫持检测" desc="系统 DNS 与多家公共 DNS 交叉验证,识别劫持、污染或运营商改写" running={dnsConsRun} onRun={runDnsCons}>
          <div className="row mb8">
            <input className="text-input grow" value={dnsDomain} onChange={(e) => setDnsDomain(e.target.value)} placeholder="要检测的域名" />
          </div>
          {dnsCons && (
            <>
              <div className="row mb8">
                <span className={`chip ${dnsCons.verdict === "一致" ? "green" : dnsCons.verdict === "可疑劫持" ? "red" : "amber"}`}>{dnsCons.verdict}</span>
                {dnsCons.note && <span className="faint" style={{ fontSize: 11 }}>{dnsCons.note}</span>}
              </div>
              {dnsCons.entries.map((e) => (
                <ResultRow
                  key={`${e.source}-${e.server}`}
                  label={`${e.source} DNS ${e.server}`}
                  s={e.ok && e.ips.length ? "ok" : e.ok ? "warn" : "err"}
                  value={e.ok ? `${e.rtt_ms?.toFixed(1)} ms · ${e.ips.join(", ")}` : (e.error ?? "失败")}
                />
              ))}
            </>
          )}
        </ActionCard>

        <ActionCard icon="wifi" title="NAT 类型检测(STUN)" desc="RFC 5389 标准探测,判定公网直连/锥形/对称 NAT(简化判定)" running={natRun} onRun={runNat} status={nat ? (nat.servers.some((s) => s.ok) ? "ok" : "unknown") : undefined}>
          {nat && (
            <>
              <ResultRow label="NAT 类型" s={nat.nat_type === "公网直连" ? "ok" : nat.nat_type === "锥形 NAT" ? "ok" : nat.nat_type === "对称 NAT" ? "warn" : "unknown"} value={nat.nat_type} />
              <ResultRow label="本机地址" value={nat.local_ip ?? "未知"} />
              {nat.servers.map((s) => (
                <ResultRow key={s.server} label={`STUN ${s.server}`} s={s.ok ? "ok" : "err"} value={s.ok ? `映射 ${s.mapped_ip}:${s.mapped_port}` : (s.error ?? "失败")} />
              ))}
              {nat.note && <div className="faint mt8" style={{ fontSize: 11.5 }}>{nat.note}</div>}
            </>
          )}
        </ActionCard>

        <ActionCard icon="globe" title="服务连通性矩阵" desc="并发探测常用服务 TCP 443(无法连接可能为本地网络策略,不代表服务不可用)" running={gridRun} onRun={runGrid}
                    status={grid ? (grid.every((r) => r.ok) ? "ok" : grid.some((r) => r.ok) ? "warn" : "err") : undefined}>
          {grid && (
            <div className="connect-grid">
              {grid.map((r) => (
                <div key={r.name} className={`connect-cell ${r.ok ? "ok" : "err"}`}>
                  <span className="connect-name">{r.name}</span>
                  <span className="connect-ms mono">{r.ok ? `${r.ms?.toFixed(0)} ms` : "不可达"}</span>
                </div>
              ))}
            </div>
          )}
        </ActionCard>

        <ActionCard icon="search" title="自定义 ICMP Ping" desc="对任意主机执行真实 ICMP 检测(8 个数据包)" running={pingRun} onRun={runPing}>
          <div className="row mb8">
            <input className="text-input grow" value={pingHost} onChange={(e) => setPingHost(e.target.value)} placeholder="主机名或 IP" />
          </div>
          {ping && (
            <>
              <ResultRow label="丢包率" s={ping.loss_percent != null ? (ping.loss_percent === 0 ? "ok" : ping.loss_percent < 50 ? "warn" : "err") : "unknown"} value={ping.loss_percent != null ? `${ping.loss_percent}%` : "未知"} />
              <ResultRow label="平均延迟" value={ping.avg_ms != null ? `${ping.avg_ms.toFixed(1)} ms` : "无响应"} />
              {ping.rtts_ms.length > 0 && (
                <ResultRow label="各包延迟" value={ping.rtts_ms.map((r) => r.toFixed(0)).join(" / ") + " ms"} />
              )}
            </>
          )}
        </ActionCard>
      </div>

      {/* 路由追踪 */}
      <div className="card pad mt16">
        <div className="row">
          <div className="chart-title grow" style={{ margin: 0 }}><Icon name="trending" size={15} /> 路由追踪 (tracert)</div>
          <input className="text-input" style={{ width: 240 }} value={traceHost} onChange={(e) => setTraceHost(e.target.value)} placeholder="目标主机" />
          <button className="btn sm" onClick={runTrace} disabled={traceRun}>
            {traceRun ? <Icon name="refresh" size={13} className="spin" /> : <Icon name="play" size={13} />}
            {traceRun ? "追踪中(最长约 1 分钟)" : "开始追踪"}
          </button>
        </div>
        {trace && trace.length > 0 && (
          <div className="table-wrap mt12" style={{ maxHeight: 300, overflowY: "auto" }}>
            <table className="table">
              <thead><tr><th>跳数</th><th>IP 地址</th><th>延迟 (ms)</th><th>状态</th></tr></thead>
              <tbody>
                {trace.map((h) => (
                  <tr key={h.hop}>
                    <td className="num">{h.hop}</td>
                    <td className="num">{h.ip ?? "*"}</td>
                    <td className="num">{h.times_ms.length ? h.times_ms.map((t) => t.toFixed(0)).join(" / ") : "—"}</td>
                    <td>{h.timeout ? <span className="chip amber">超时</span> : <span className="chip green">到达</span>}</td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        )}
        {trace && trace.length === 0 && <div className="empty">未能解析出任何路由跳点(可能被防火墙拦截)</div>}
        {!trace && !traceRun && <div className="faint mt8" style={{ fontSize: 11.5 }}>追踪使用系统 tracert,目标不可达时对应跳显示为超时。</div>}
      </div>
    </div>
  );
}

function IfaceView({ i, wifi, compact }: { i: IfaceInfo; wifi: WifiDetails | null; compact?: boolean }) {
  const kindText = i.kind === "wifi" ? "Wi-Fi" : i.kind === "ethernet" ? "以太网" : i.kind === "cellular" ? "移动网络" : "其他";
  const link = i.link_mbps != null ? (i.link_mbps >= 1000 ? `${(i.link_mbps / 1000).toFixed(1)} Gbps` : `${i.link_mbps} Mbps`) : null;
  return (
    <div className={compact ? "iface-block compact" : "iface-block"}>
      <div className="row gap8">
        <Dot s={i.oper_up ? "ok" : "unknown"} />
        <b style={{ fontSize: 13 }}>{i.friendly_name ?? i.name}</b>
        <span className="chip">{kindText}</span>
        {i.is_default && <span className="chip cyan">默认</span>}
        {link && <span className="chip">{link}(网卡协商速率)</span>}
      </div>
      {!compact && (
        <>
          <ResultRow label="网卡型号" value={i.description ?? "系统未提供"} />
          <ResultRow label="MAC 地址" value={i.mac ?? "未知"} />
          <ResultRow label="IPv4" value={i.ipv4.join(", ") || "无"} s={i.ipv4.length ? "ok" : "unknown"} />
          <ResultRow label="IPv6" value={i.ipv6.join(", ") || "无"} s={i.ipv6.length ? "ok" : "unknown"} />
          <ResultRow label="默认网关" value={i.gateway ?? "无"} s={i.gateway ? "ok" : "unknown"} />
          <ResultRow label="DNS 服务器" value={i.dns_servers.join(", ") || "未获取"} s={i.dns_servers.length ? "ok" : "unknown"} />
          <ResultRow label="MTU" value={i.mtu != null ? String(i.mtu) : "未知"} />
          {i.kind === "wifi" && wifi && (
            <>
              <ResultRow label="Wi-Fi SSID" value={wifi.ssid ?? "未知"} />
              <ResultRow
                label="信号强度"
                s={wifi.signal_percent == null ? "unknown" : wifi.signal_percent >= 70 ? "ok" : wifi.signal_percent >= 40 ? "warn" : "err"}
                value={wifi.signal_percent != null ? `${wifi.signal_percent}%` : "硬件不支持"}
              />
              <ResultRow label="无线电类型 / 信道" value={[wifi.radio_type, wifi.channel].filter(Boolean).join(" · ") || "未知"} />
              <ResultRow label="协商收发速率" value={[wifi.receive_rate_mbps, wifi.transmit_rate_mbps].every((v) => v == null) ? "未知" : `收 ${wifi.receive_rate_mbps?.toFixed(0) ?? "?"} / 发 ${wifi.transmit_rate_mbps?.toFixed(0) ?? "?"} Mbps`} />
            </>
          )}
        </>
      )}
    </div>
  );
}

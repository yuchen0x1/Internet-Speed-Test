// 批量拨测(ITDOG 式):多目标并发真实 ICMP,ITDOG 同款配色/聚合表/走势,
// 外加域名解析分布(多 DNS 交叉统计)。
import { useEffect, useMemo } from "react";
import { api } from "../api";
import { useApp } from "../store/app";
import { getBatchSamples, useBatch } from "../store/batch";
import { Icon } from "../components/Icon";

/** ITDOG 延迟配色档位(颜色函数与图例共用同一定义,数值勿单独改动) */
const LAT_BANDS = [
  { max: 50, color: "var(--green)", label: "≤50ms" },
  { max: 100, color: "#22d3ee", label: "≤100ms" },
  { max: 200, color: "var(--amber)", label: "≤200ms" },
  { max: 250, color: "#fb923c", label: "≤250ms" },
] as const;

/** 超过最后一档或超时 → 红 */
function latColor(ms: number | null | undefined): string {
  if (ms == null) return "var(--red)";
  for (const b of LAT_BANDS) if (ms <= b.max) return b.color;
  return "var(--red)";
}

function latChip(ms: number | null | undefined): string {
  if (ms == null) return "超时";
  return `${ms.toFixed(0)} ms`;
}

/** 迷你走势图(真实样本序列) */
function Sparkline(props: { samples: (number | null)[]; width?: number; height?: number }) {
  const { samples, width = 150, height = 26 } = props;
  const vals = samples.slice(-40);
  const ok = vals.filter((v): v is number => v != null);
  const max = Math.max(1, ...ok);
  const pts: string[] = [];
  vals.forEach((v, i) => {
    if (v == null) return;
    const x = (i / Math.max(1, vals.length - 1)) * (width - 4) + 2;
    const y = height - 3 - (v / max) * (height - 8);
    pts.push(`${x.toFixed(1)},${y.toFixed(1)}`);
  });
  return (
    <svg width={width} height={height} style={{ display: "block" }}>
      <line x1="0" y1={height - 2} x2={width} y2={height - 2} stroke="rgba(122,176,255,.15)" />
      {pts.length > 1 && (
        <polyline points={pts.join(" ")} fill="none" stroke="var(--green)" strokeWidth="1.4" />
      )}
      {pts.length > 0 && (() => {
        const [x, y] = pts[pts.length - 1].split(",");
        return <circle cx={x} cy={y} r="2.2" fill="var(--green)" />;
      })()}
    </svg>
  );
}

export function BatchTestPage() {
  const toast = useApp((s) => s.toast);
  // 拨测进度/结果/样本存 store:切页回来不丢(见 store/batch.ts)
  const {
    targetsText, count, running, total, doneCount, results, samplesVer,
    dnsDomain, dnsDist, dnsRun,
    setTargetsText, setCount, setRunning, prepareRun, setDnsDomain, setDnsDist, setDnsRun,
    bind,
  } = useBatch();

  // 监听在 store 层绑定一次且不随页面卸载解绑:进行中切页不丢 tick
  useEffect(() => {
    void bind();
  }, [bind]);

  const start = async () => {
    const targets = targetsText.split("\n").map((t) => t.trim()).filter(Boolean);
    if (!targets.length) {
      toast("warn", "请至少输入一个目标");
      return;
    }
    try {
      const n = await api.batchtestStart(targets, count);
      prepareRun(n * count);
      setRunning(true);
      toast("info", `批量拨测已启动:${n} 个目标 × ${count} 包`);
    } catch (e) {
      toast("error", String(e));
    }
  };

  const stop = async () => {
    await api.batchtestStop();
    setRunning(false);
    toast("warn", "批量拨测已停止");
  };

  const rows = useMemo(() => {
    // 有汇总结果用汇总;进行中的目标用实时样本计算
    const list: {
      target: string; latest: number | null; min: number | null; max: number | null;
      avg: number | null; loss: number; sent: number; samples: (number | null)[];
    }[] = [];
    for (const r of results) {
      list.push({
        target: r.target, latest: r.latest_ms, min: r.min_ms, max: r.max_ms,
        avg: r.avg_ms, loss: r.loss_percent, sent: r.sent, samples: r.samples,
      });
    }
    for (const [target, s] of getBatchSamples()) {
      if (results.some((r) => r.target === target)) continue;
      const ok = s.filter((v): v is number => v != null);
      list.push({
        target,
        latest: s[s.length - 1] ?? null,
        min: ok.length ? Math.min(...ok) : null,
        max: ok.length ? Math.max(...ok) : null,
        avg: ok.length ? ok.reduce((a, b) => a + b, 0) / ok.length : null,
        loss: s.length ? ((s.length - ok.length) / s.length) * 100 : 0,
        sent: s.length,
        samples: s,
      });
    }
    return list.sort((a, b) => (a.avg ?? 9e9) - (b.avg ?? 9e9));
  }, [results, samplesVer]);

  const overall = useMemo(() => {
    const withAvg = rows.filter((r) => r.avg != null);
    const fastest = withAvg[0];
    const slowest = withAvg[withAvg.length - 1];
    const avg = withAvg.length
      ? withAvg.reduce((a, r) => a + (r.avg ?? 0), 0) / withAvg.length
      : null;
    return { fastest, slowest, avg };
  }, [rows]);

  const queryDns = async () => {
    setDnsRun(true);
    setDnsDist(null);
    try {
      setDnsDist(await api.dnsDistribution(dnsDomain.trim()));
    } catch (e) {
      toast("error", String(e));
    } finally {
      setDnsRun(false);
    }
  };

  const progress = total > 0 ? Math.min(100, (doneCount / total) * 100) : 0;

  return (
    <div>
      <div className="page-head">
        <div>
          <h1 className="page-title">
            <span className="icon-wrap"><Icon name="layers" size={22} /></span>
            批量拨测
          </h1>
          <div className="page-sub">多目标并发真实 ICMP · ITDOG 同款延迟配色与聚合表 · 本机作为拨测源</div>
        </div>
        <div className="row gap8">
          <button className="btn primary" onClick={start} disabled={running}>
            <Icon name="play" size={14} /> 开始拨测
          </button>
          <button className="btn danger" onClick={stop} disabled={!running}>
            <Icon name="stop" size={14} /> 停止
          </button>
        </div>
      </div>

      {/* 目标配置 */}
      <div className="card pad mb16">
        <div className="row" style={{ alignItems: "flex-start" }}>
          <div className="grow">
            <div className="chart-title">目标列表(每行一个主机名或 IP)</div>
            <textarea
              className="text-input mono"
              style={{ width: "100%", height: 108, resize: "vertical", lineHeight: 1.7 }}
              value={targetsText}
              onChange={(e) => setTargetsText(e.target.value)}
              spellCheck={false}
            />
          </div>
          <div style={{ width: 170, flexShrink: 0 }}>
            <div className="chart-title">每目标发包数</div>
            <select className="select" style={{ width: "100%" }} value={count}
                    onChange={(e) => setCount(Number(e.target.value))} disabled={running}>
              <option value={10}>10 包</option>
              <option value={20}>20 包</option>
              <option value={50}>50 包</option>
            </select>
            <div className="faint mt8" style={{ fontSize: 11, lineHeight: 1.7 }}>
              并发探测全部目标,包间隔 250ms。
              <br />
              颜色规则:
              <br />
              {LAT_BANDS.map((b) => (
                <span key={b.max}>
                  <span style={{ color: b.color }}>■</span> {b.label}
                  {" "}
                </span>
              ))}
              <span style={{ color: "var(--red)" }}>■</span> 更高/超时
            </div>
          </div>
        </div>
        {(running || doneCount > 0) && (
          <div className="mt12">
            <div className="row" style={{ fontSize: 12, marginBottom: 5 }}>
              <span className="dim">当前进度</span>
              <div className="grow" />
              <b className="mono">{doneCount}/{total}</b>
            </div>
            <div className="progress"><div style={{ width: `${progress}%` }} /></div>
          </div>
        )}
      </div>

      {/* 聚合摘要 */}
      {rows.length > 0 && (
        <div className="card pad mb16">
          <div className="chart-title">区域聚合(ITDOG 同款:最快 / 最慢 / 平均)</div>
          <div className="row gap8 wrap">
            <span className="chip green">最快:{overall.fastest ? `${overall.fastest.target} ${latChip(overall.fastest.avg)}` : "—"}</span>
            <span className="chip red">最慢:{overall.slowest ? `${overall.slowest.target} ${latChip(overall.slowest.avg)}` : "—"}</span>
            <span className="chip cyan">整体平均:{overall.avg != null ? latChip(overall.avg) : "—"}</span>
            <span className="chip">目标数 {rows.length}</span>
          </div>
        </div>
      )}

      {/* 结果表 */}
      {rows.length > 0 && (
        <div className="card" style={{ padding: 0 }}>
          <div className="table-wrap" style={{ maxHeight: 420, overflowY: "auto" }}>
            <table className="table">
              <thead>
                <tr>
                  <th>目标</th><th>最新</th><th>最快</th><th>最慢</th><th>平均</th>
                  <th>发包</th><th>丢包</th><th>走势</th>
                </tr>
              </thead>
              <tbody>
                {rows.map((r) => (
                  <tr key={r.target}>
                    <td>
                      <div className="row gap8">
                        <span className="status-dot" style={{ background: latColor(r.avg), boxShadow: `0 0 8px ${latColor(r.avg)}` }} />
                        <b className="mono" style={{ fontSize: 12.5 }}>{r.target}</b>
                      </div>
                    </td>
                    <td className="num" style={{ color: latColor(r.latest) }}>{latChip(r.latest)}</td>
                    <td className="num">{r.min != null ? `${r.min.toFixed(0)} ms` : "—"}</td>
                    <td className="num">{r.max != null ? `${r.max.toFixed(0)} ms` : "—"}</td>
                    <td className="num" style={{ color: latColor(r.avg), fontWeight: 700 }}>{latChip(r.avg)}</td>
                    <td className="num">{r.sent}</td>
                    <td className="num">
                      <span className={`chip ${r.loss > 0 ? (r.loss >= 100 ? "red" : "amber") : "green"}`}>
                        {r.loss.toFixed(0)}%
                      </span>
                    </td>
                    <td><Sparkline samples={r.samples} /></td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        </div>
      )}

      {/* 域名解析分布 */}
      <div className="card pad mt16">
        <div className="row">
          <div className="chart-title grow" style={{ margin: 0 }}>
            <Icon name="globe" size={15} /> 域名解析分布(系统 + 4 家公共 DNS 交叉统计)
          </div>
          <input className="text-input" style={{ width: 240 }} value={dnsDomain}
                 onChange={(e) => setDnsDomain(e.target.value)} placeholder="要查询的域名" />
          <button className="btn sm" onClick={queryDns} disabled={dnsRun}>
            <Icon name="search" size={13} /> {dnsRun ? "查询中" : "查询分布"}
          </button>
        </div>
        {dnsDist && (
          <>
            {dnsDist.distribution.length > 0 ? (
              <div className="row gap8 wrap mt12">
                {dnsDist.distribution.map(([ip, cnt]) => (
                  <span key={ip} className="chip cyan">
                    <span className="mono">{ip}</span>
                    <b>{((cnt / Math.max(1, dnsDist.resolvers)) * 100).toFixed(0)}%</b>
                  </span>
                ))}
              </div>
            ) : (
              <div className="faint mt8" style={{ fontSize: 11.5 }}>所有解析器均无响应。</div>
            )}
            <div className="faint mt8" style={{ fontSize: 11.5 }}>
              {dnsDist.resolvers} 家解析器响应 · 不同 DNS 返回不同 IP 属于 CDN 地域调度;若出现内网/保留地址则提示劫持风险。
            </div>
          </>
        )}
      </div>
    </div>
  );
}

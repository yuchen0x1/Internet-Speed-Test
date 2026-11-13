// 批量拨测(ITDOG 式):多目标并发真实 ICMP,ITDOG 同款配色/聚合表/走势,
// 外加域名解析分布(多 DNS 交叉统计)。
import { useEffect, useMemo, useState } from "react";
import { api, errMsg } from "../api";
import { useApp } from "../store/app";
import { useBatch } from "../store/batch";
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
  const segments: string[][] = [];
  const missing: number[] = [];
  let segment: string[] = [];
  vals.forEach((v, i) => {
    const x = (i / Math.max(1, vals.length - 1)) * (width - 4) + 2;
    if (v == null) {
      if (segment.length) segments.push(segment);
      segment = [];
      missing.push(x);
      return;
    }
    const y = height - 3 - (v / max) * (height - 8);
    segment.push(`${x.toFixed(1)},${y.toFixed(1)}`);
  });
  if (segment.length) segments.push(segment);
  return (
    <svg width={width} height={height} style={{ display: "block" }}>
      <line x1="0" y1={height - 2} x2={width} y2={height - 2} stroke="rgba(122,176,255,.15)" />
      {segments.map((points, i) => <g key={i}>
        {points.length > 1 && <polyline points={points.join(" ")} fill="none" stroke="var(--green)" strokeWidth="1.4" />}
        {points.map((point, j) => {
          const [x, y] = point.split(",");
          return <circle key={j} cx={x} cy={y} r="1.8" fill="var(--green)" />;
        })}
      </g>)}
      {missing.map((x, i) => <circle key={i} cx={x} cy={height - 3} r="2" fill="var(--red)" />)}
    </svg>
  );
}

export function BatchTestPage() {
  const toast = useApp((s) => s.toast);
  // 拨测进度/结果/样本存 store:切页回来不丢(见 store/batch.ts)
  const {
    targetsText, count, running, busy, bound, error, snapshot, activeTargets, total, doneCount, results,
    dnsDomain, dnsDist, dnsRun,
    setTargetsText, setCount, start, stop, setDnsDomain, setDnsDist, setDnsRun,
    bind,
  } = useBatch();

  // 监听在 store 层绑定一次且不随页面卸载解绑:进行中切页不丢 tick
  useEffect(() => {
    void bind();
  }, [bind]);

  const [exporting, setExporting] = useState(false);
  const exportResults = async (format: "csv" | "json") => {
    if (!snapshot || exporting) return;
    setExporting(true);
    try {
      const path = await api.batchtestExport(snapshot.run_id, format);
      toast("success", `已导出拨测结果：${path}`);
    } catch (e) { if (errMsg(e) !== "已取消保存") toast("error", errMsg(e)); }
    finally { setExporting(false); }
  };

  const rows = useMemo(() => activeTargets.map((target) => {
    const r = results.find((r) => r.target === target);
    return {
      target, latest: r?.latest_ms ?? null, min: r?.min_ms ?? null, max: r?.max_ms ?? null,
      avg: r?.avg_ms ?? null, loss: r?.loss_percent ?? 0, sent: r?.sent ?? 0, samples: r?.samples ?? [],
    };
  }).sort((a, b) => (a.avg ?? Infinity) - (b.avg ?? Infinity) || a.target.localeCompare(b.target)), [results, activeTargets]);

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
          <div className="page-sub">多目标真实 ICMP · 最多 32 个目标 · 8 路并发 · 本机作为拨测源</div>
        </div>
        <div className="row gap8">
          <button className="btn primary" onClick={start} disabled={running || busy || !bound}>
            <Icon name="play" size={14} /> {busy && !running ? "连接中…" : "开始拨测"}
          </button>
          <button className="btn danger" onClick={stop} disabled={!running || busy || snapshot?.cancelled}>
            <Icon name="stop" size={14} /> {snapshot?.cancelled && running ? "停止中…" : "停止"}
          </button>
        </div>
      </div>

      {error && <div className="card pad mb16" role="alert">
        {error} <button className="btn sm" onClick={() => { void (bound ? useBatch.getState().sync() : bind()).catch((e) => toast("error", errMsg(e))); }}>重新连接</button>
      </div>}
      {/* 目标配置 */}
      <div className="card pad mb16">
        <div className="row" style={{ alignItems: "flex-start" }}>
          <div className="grow">
            <label htmlFor="batch-targets" className="chart-title">目标列表（每行一个主机名或 IP，自动去重）</label>
            <textarea
              id="batch-targets"
              disabled={running || busy}
              className="text-input mono"
              style={{ width: "100%", height: 108, resize: "vertical", lineHeight: 1.7 }}
              value={targetsText}
              onChange={(e) => setTargetsText(e.target.value)}
              spellCheck={false}
            />
          </div>
          <div style={{ width: 170, flexShrink: 0 }}>
            <label htmlFor="batch-count" className="chart-title">每目标发包数</label>
            <select id="batch-count" className="select" style={{ width: "100%" }} value={count}
                    onChange={(e) => setCount(Number(e.target.value))} disabled={running || busy}>
              <option value={10}>10 包</option>
              <option value={20}>20 包</option>
              <option value={50}>50 包</option>
            </select>
            <div className="faint mt8" style={{ fontSize: 11, lineHeight: 1.7 }}>
              每次探测最多 8 个目标，其余排队；包间隔 250ms。
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
        {snapshot && (
          <div className="mt12">
            <div className="row" style={{ fontSize: 12, marginBottom: 5 }}>
              <span className="dim">{running ? (snapshot.cancelled ? "正在停止" : "正在拨测") : snapshot.cancelled ? "已停止 · 保留部分结果" : "拨测完成"}</span>
              <div className="grow" />
              <b className="mono">{doneCount}/{total} 包</b>
            </div>
            <div className="progress" role="progressbar" aria-label="拨测发包进度" aria-valuemin={0} aria-valuemax={total} aria-valuenow={doneCount}><div style={{ width: `${progress}%` }} /></div>
          </div>
        )}
      </div>

      {/* 聚合摘要 */}
      {rows.length > 0 && (
        <div className="card pad mb16">
          <div className="row gap8 wrap mb12">
            <div className="chart-title grow" style={{ margin: 0 }}>目标汇总</div>
            <button className="btn sm" disabled={running || busy || exporting} onClick={() => exportResults("csv")}>导出 CSV</button>
            <button className="btn sm" disabled={running || busy || exporting} onClick={() => exportResults("json")}>导出 JSON</button>
          </div>
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
                        <span className="status-dot" style={{ background: r.sent ? latColor(r.avg) : "var(--text-faint)" }} />
                        <b className="mono" style={{ fontSize: 12.5 }}>{r.target}</b>
                      </div>
                    </td>
                    <td className="num" style={{ color: latColor(r.latest) }}>{r.sent ? latChip(r.latest) : "—"}</td>
                    <td className="num">{r.min != null ? `${r.min.toFixed(0)} ms` : "—"}</td>
                    <td className="num">{r.max != null ? `${r.max.toFixed(0)} ms` : "—"}</td>
                    <td className="num" style={{ color: latColor(r.avg), fontWeight: 700 }}>{r.sent ? latChip(r.avg) : running ? "排队中" : "未探测"}</td>
                    <td className="num">{r.sent}</td>
                    <td className="num">
                      <span className={`chip ${!r.sent ? "" : r.loss > 0 ? (r.loss >= 100 ? "red" : "amber") : "green"}`}>
                        {r.sent ? `${r.loss.toFixed(0)}%` : "—"}
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

      {!snapshot && <div className="card pad mb16 dim">输入需要比较的主机并开始拨测。超时会保留为缺失样本；停止后仍可导出已完成部分。</div>}
      {/* 域名解析分布 */}
      <div className="card pad mt16">
        <div className="row">
          <div className="chart-title grow" style={{ margin: 0 }}>
            <Icon name="globe" size={15} /> 域名解析分布(系统 + 4 家公共 DNS 交叉统计)
          </div>
          <input className="text-input" aria-label="查询解析分布的域名" style={{ width: 240 }} value={dnsDomain}
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
              {dnsDist.resolvers} 家解析器响应 · 不同 DNS 返回不同 IP 可能来自 CDN 调度；如需排查劫持，请使用网络诊断中的 DNS 一致性检测。
            </div>
          </>
        )}
      </div>
    </div>
  );
}

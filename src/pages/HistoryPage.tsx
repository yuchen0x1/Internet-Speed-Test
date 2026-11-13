// 测速历史:筛选/排序/对比/趋势/导出/报告/删除
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { api } from "../api";
import { useApp } from "../store/app";
import { useTest } from "../store/test";
import { useHistoryUi } from "../store/ui";
import { isCancelError, latencyPoints } from "../lib/format";
import { historyDateRange, recentCompleted } from "../lib/history";
import { LineChart, type Series } from "../components/LineChart";
import { HistoryHeatmap } from "../components/HistoryHeatmap";
import { ConfirmModal, Modal } from "../components/ui";
import { Icon } from "../components/Icon";
import type { HistoryRow, HistorySummary, TestResult } from "../types";

const HISTORY_PAGE_SIZE = 100;

const STATUS_CHIP: Record<string, { cls: string; text: string }> = {
  completed: { cls: "green", text: "完成" },
  cancelled: { cls: "amber", text: "已取消" },
  failed: { cls: "red", text: "失败" },
};

export function HistoryPage() {
  const toast = useApp((s) => s.toast);
  const lastResult = useTest((s) => s.result);
  // 筛选与选择提升到 store:切页回来不丢(见 store/ui.ts)
  const { from, to, desc, selected, setFrom, setTo, setRange, toggleDesc, setSelected, toggleSel } = useHistoryUi();
  const [rows, setRows] = useState<HistoryRow[]>([]);
  // 筛选条件下的真实总条数; rows 可通过游标分页逐步加载,不是全量快照
  const [total, setTotal] = useState(0);
  const [summary, setSummary] = useState<HistorySummary | null>(null);
  const [loadError, setLoadError] = useState<string | null>(null);
  const [exporting, setExporting] = useState(false);
  const invalidRange = !!from && !!to && from > to;
  // 清空是全局操作(无视筛选),确认框必须显示库内真实总数
  const [globalTotal, setGlobalTotal] = useState<number | null>(null);
  const [loading, setLoading] = useState(false);
  const [loadingMore, setLoadingMore] = useState(false);
  const [hasMore, setHasMore] = useState(false);
  const nextCursor = useRef<number | null>(null);
  const [confirmClear, setConfirmClear] = useState(false);
  const [detail, setDetail] = useState<TestResult | null>(null);
  // 请求序号:筛选/排序变更后,慢响应若已过期则丢弃,避免覆盖新结果
  const loadSeq = useRef(0);

  const openDetail = async (id: number) => {
    try {
      const d = await api.historyDetail(id);
      if (d) setDetail(d);
      else toast("warn", "该记录的完整样本数据不存在");
    } catch (e) {
      toast("error", `详情加载失败:${String(e)}`);
    }
  };

  const openClearConfirm = async () => {
    setConfirmClear(true);
    try {
      // 清空作用于整库,不受当前筛选影响:单独取一次全量计数
      setGlobalTotal(await api.historyCount());
    } catch {
      setGlobalTotal(null); // 取不到就退回已加载条数,不阻塞用户操作
    }
  };

  const load = useCallback(async () => {
    const seq = ++loadSeq.current;
    const filter = { from: from || undefined, to: to || undefined };
    setRows([]);
    setTotal(0);
    setSummary(null);
    setLoadError(null);
    setLoading(true);
    setLoadingMore(false);
    setHasMore(false);
    nextCursor.current = null;
    if (from && to && from > to) {
      setLoading(false);
      return;
    }
    try {
      // 列表与总数并发获取:同一筛选口径,避免统计窗口被误当全量
      const [list, stats] = await Promise.all([
        api.historyList({ ...filter, desc, limit: HISTORY_PAGE_SIZE + 1 }),
        api.historySummary(filter),
      ]);
      if (seq !== loadSeq.current) return; // 过期响应:丢弃,不覆盖新筛选结果
      const firstPage = list.slice(0, HISTORY_PAGE_SIZE);
      setRows(firstPage);
      setTotal(stats.total);
      setSummary(stats);
      nextCursor.current = firstPage.at(-1)?.id ?? null;
      setHasMore(list.length > HISTORY_PAGE_SIZE);
    } catch (e) {
      if (seq === loadSeq.current) setLoadError(`历史加载失败：${String(e)}`);
    } finally {
      if (seq === loadSeq.current) setLoading(false);
    }
  }, [from, to, desc, toast]);

  const loadMore = useCallback(async () => {
    if (loading || loadingMore || !hasMore) return;
    const seq = loadSeq.current;
    const cursorId = nextCursor.current;
    setLoadingMore(true);
    try {
      const next = await api.historyList({
        from: from || undefined,
        to: to || undefined,
        desc,
        limit: HISTORY_PAGE_SIZE + 1,
        cursorId: cursorId ?? undefined,
      });
      if (seq !== loadSeq.current) return;
      const page = next.slice(0, HISTORY_PAGE_SIZE);
      nextCursor.current = page.at(-1)?.id ?? cursorId;
      setHasMore(next.length > HISTORY_PAGE_SIZE);
      setRows((current) => {
        const currentIds = new Set(current.map((row) => row.id));
        return [...current, ...page.filter((row) => !currentIds.has(row.id))];
      });
    } catch (e) {
      if (seq === loadSeq.current) toast("error", `更多历史加载失败:${String(e)}`);
    } finally {
      if (seq === loadSeq.current) setLoadingMore(false);
    }
  }, [from, to, desc, loading, loadingMore, hasMore, toast]);

  useEffect(() => {
    load();
    return () => { loadSeq.current++; };
  }, [load, lastResult?.id]);

  const stats = useMemo(() => {
    const done = rows.filter((r) => r.status === "completed");
    const byDay = new Map<string, { sum: number; n: number }>();
    for (const r of done) {
      const k = r.created_at.slice(0, 10);
      const e = byDay.get(k) ?? { sum: 0, n: 0 };
      e.sum += r.download_mbps;
      e.n += 1;
      byDay.set(k, e);
    }
    const daily = [...byDay.entries()]
      .map(([k, v]) => ({ day: k, avg: v.sum / v.n }))
      .sort((a, b) => a.day.localeCompare(b.day))
      .slice(-14);
    return { daily };
  }, [rows]);

  const trendData = useMemo(() => {
    const asc = recentCompleted(rows);
    const down: Series = {
      name: "下载", color: "#22d3ee",
      data: asc.map((r, i) => [i, r.download_mbps] as [number, number]),
      fill: true,
    };
    const up: Series = {
      name: "上传", color: "#a78bfa",
      data: asc.flatMap((r, i) => r.upload_mbps == null ? [] : [[i, r.upload_mbps] as [number, number]]),
    };
    return { down, up, n: asc.length };
  }, [rows]);

  // series 数组引用稳定,避免 LineChart 无谓重绘
  const trendSeries = useMemo<Series[]>(() => [trendData.down, trendData.up], [trendData]);

  const compareRows = rows.filter((r) => selected.includes(r.id));

  const doExport = async (format: "csv" | "json") => {
    if (invalidRange || exporting) return;
    setExporting(true);
    try {
      const r = await api.historyExport(format, { from: from || undefined, to: to || undefined, desc });
      if (r.total > r.exported) {
        toast("warn", `已导出前 ${r.exported} 条(共 ${r.total} 条,超出上限部分被截断):${r.path}`);
      } else {
        toast("success", `已导出:${r.path}`);
      }
    } catch (e) {
      if (!isCancelError(e)) toast("error", `导出失败:${String(e)}`);
    } finally {
      setExporting(false);
    }
  };
  const doReport = async (id: number) => {
    try {
      const path = await api.historyExportReport(id);
      toast("success", `报告已保存:${path}`);
    } catch (e) {
      if (!isCancelError(e)) toast("error", `报告生成失败:${String(e)}`);
    }
  };
  const doDelete = async (id: number) => {
    try {
      await api.historyDelete(id);
      setSelected(selected.filter((x) => x !== id));
      load();
    } catch (e) {
      toast("error", String(e));
    }
  };

  return (
    <div>
      <div className="page-head">
        <div>
          <h1 className="page-title">
            <span className="icon-wrap"><Icon name="history" size={22} /></span>
            测速历史
          </h1>
          <div className="page-sub">全部数据来自本机实际执行的测试 · 本地 SQLite 存储</div>
        </div>
        <div className="row gap8 wrap">
          <button className="btn sm" onClick={load} disabled={loading}>{loading ? "正在刷新…" : "刷新记录"}</button>
          <button className="btn sm" onClick={toggleDesc}>
            <Icon name="clock" size={13} /> {desc ? "最新在前" : "最早在前"}
          </button>
          <button className="btn sm danger" onClick={openClearConfirm}><Icon name="trash" size={13} /> 清空</button>
        </div>
      </div>

      <div className="card pad mb16 history-toolbar" role="group" aria-label="历史日期筛选和导出">
        <div className="row gap8 wrap">
          <label className="history-date-label">起始日期
            <input type="date" className="text-input" value={from} onChange={(e) => setFrom(e.target.value)} aria-invalid={invalidRange} />
          </label>
          <label className="history-date-label">结束日期
            <input type="date" className="text-input" value={to} onChange={(e) => setTo(e.target.value)} aria-invalid={invalidRange} />
          </label>
          <div className="row gap4 wrap" aria-label="快捷日期范围">
            {([{ days: 1, label: "今天" }, { days: 7, label: "近 7 天" }, { days: 30, label: "近 30 天" }]).map(({ days, label }) => (
              <button className="btn sm" key={days} onClick={() => {
                const range = historyDateRange(days);
                setRange(range.from, range.to);
              }}>{label}</button>
            ))}
            <button className="btn ghost sm" onClick={() => setRange("", "")}>全部日期</button>
          </div>
        </div>
        <div className="row gap8 wrap">
          <span className="faint">{exporting ? "正在导出…" : "导出当前筛选"}</span>
          <button className="btn sm" disabled={invalidRange || loading || exporting || !total} onClick={() => doExport("csv")}><Icon name="export" size={13} /> CSV</button>
          <button className="btn sm" disabled={invalidRange || loading || exporting || !total} onClick={() => doExport("json")}><Icon name="export" size={13} /> JSON</button>
        </div>
      </div>
      {(invalidRange || loadError) && (
        <div className="test-error mb16" role="alert">
          {invalidRange ? "起始日期不能晚于结束日期，请调整日期范围。" : loadError}
          {!invalidRange && <button className="btn sm" onClick={load}>重试加载</button>}
        </div>
      )}

      {/* 统计覆盖整个筛选范围，图表仍如实说明已加载窗口。 */}
      {total > 0 && (
        <div className="card pad mb16" style={{ padding: "10px 14px" }}>
          <div className="row gap12 wrap" style={{ justifyContent: "space-between" }}>
            <span className="faint" style={{ fontSize: 11.5 }}>
              当前筛选共 <b className="mono" style={{ color: "var(--text)" }}>{total}</b> 条,
              已加载 <b className="mono" style={{ color: "var(--text)" }}>{rows.length}</b> 条;
              统计卡覆盖整个筛选范围；日均对比、趋势与热力图基于已加载记录。
              {summary && ` 完成 ${summary.completed} · 失败 ${summary.failed} · 取消 ${summary.cancelled}。`}
            </span>
            {hasMore && (
              <button className="btn sm" onClick={loadMore} disabled={loading || loadingMore}>
                {loadingMore ? "正在加载…" : `加载更多 (约 ${Math.max(0, total - rows.length)} 条未加载)`}
              </button>
            )}
          </div>
        </div>
      )}

      {/* 统计卡行 */}
      {summary && total > 0 && (
        <div className="stat-cards-row mb16">
          <div className="card pad"><div className="stat-head"><Icon name="history" size={15} /> 成功测试</div>
            <div className="stat-value mono" style={{ fontSize: 24 }}>{summary.completed}</div><div className="stat-unit">次完成 · 全部筛选记录</div></div>
          <div className="card pad"><div className="stat-head"><Icon name="download" size={15} /> 平均下载</div>
            <div className="stat-value mono" style={{ fontSize: 24, color: "var(--cyan)" }}>{summary.avg_download_mbps?.toFixed(1) ?? "—"}</div><div className="stat-unit">Mbps · 成功测试</div></div>
          <div className="card pad"><div className="stat-head"><Icon name="upload" size={15} /> 平均上传</div>
            <div className="stat-value mono" style={{ fontSize: 24, color: "var(--purple)" }}>{summary.avg_upload_mbps?.toFixed(1) ?? "—"}</div><div className="stat-unit">Mbps · 仅有效上传结果</div></div>
          <div className="card pad"><div className="stat-head"><Icon name="target" size={15} /> 平均延迟</div>
            <div className="stat-value mono" style={{ fontSize: 24 }}>{summary.avg_ping_ms?.toFixed(1) ?? "—"}</div><div className="stat-unit">ms · 成功测试</div></div>
          <div className="card pad"><div className="stat-head"><Icon name="zap" size={15} /> 最佳下载</div>
            <div className="stat-value mono" style={{ fontSize: 24, color: "var(--green)" }}>{summary.best_download_mbps?.toFixed(1) ?? "—"}</div>
            <div className="stat-unit">Mbps · 整个筛选范围</div></div>
        </div>
      )}
      {stats.daily.length > 1 && (
        <div className="card pad mb16">
          <div className="chart-title">已加载记录 · 日均下载对比({stats.daily.length} 个测试日)</div>
          <div style={{ display: "flex", alignItems: "flex-end", gap: 6, height: 110, padding: "0 4px" }}>
            {(() => {
              const max = Math.max(...stats.daily.map((d) => d.avg), 1);
              return stats.daily.map((d) => (
                <div key={d.day} className="grow" style={{ textAlign: "center", cursor: "default" }}
                     title={`${d.day} · 日均 ${d.avg.toFixed(1)} Mbps`}>
                  <div style={{ fontSize: 10.5, color: "var(--cyan)", fontFamily: "var(--mono)", marginBottom: 3 }}>
                    {d.avg.toFixed(0)}
                  </div>
                  <div style={{
                    height: Math.max(4, (d.avg / max) * 82),
                    borderRadius: "5px 5px 2px 2px",
                    background: "linear-gradient(180deg, var(--cyan), rgba(34,211,238,.25))",
                    boxShadow: "0 0 10px rgba(34,211,238,.35)",
                  }} />
                  <div style={{ fontSize: 10, color: "var(--text-faint)", marginTop: 4 }}>{d.day.slice(5)}</div>
                </div>
              ));
            })()}
          </div>
        </div>
      )}

      {/* 日历热力图 */}
      {rows.length > 0 && (
        <div className="card pad mb16">
          <div className="chart-title">已加载记录 · 历史热力图(按天平均下载速度 · 最近 14 周)</div>
          <HistoryHeatmap rows={rows} weeks={14} />
        </div>
      )}

      {/* 趋势图 */}
      {trendData.n >= 2 && (
        <div className="card pad mb16">
          <div className="chart-title">已加载记录 · 速度趋势(最近 {trendData.n} 次成功测试，时间从左向右)</div>
          <LineChart series={trendSeries} yUnit="Mbps" avgLine height={180} emptyText="暂无足够数据" />
        </div>
      )}

      {/* 对比面板 */}
      {compareRows.length === 2 && (
        <div className="card pad mb16 compare-panel">
          <div className="chart-title"><Icon name="layers" size={15} /> 结果对比</div>
          <div className="compare-grid">
            <div />
            {compareRows.map((r) => (
              <div key={r.id} className="compare-col">
                <b className="mono" style={{ fontSize: 12 }}>#{r.id}</b>
                <div className="faint" style={{ fontSize: 11 }}>{r.created_at}</div>
              </div>
            ))}
          </div>
          {([
            ["下载 Mbps", (r: HistoryRow) => r.download_mbps.toFixed(2)],
            ["上传 Mbps", (r: HistoryRow) => r.upload_mbps?.toFixed(2) ?? "—"],
            ["空闲延迟 ms", (r: HistoryRow) => r.idle_ping_ms.toFixed(1)],
            ["抖动 ms", (r: HistoryRow) => r.jitter_ms.toFixed(1)],
            ["丢包率 %", (r: HistoryRow) => r.packet_loss_percent?.toFixed(1) ?? "—"],
            ["节点", (r: HistoryRow) => r.server_name],
          ] as const).map(([label, get]) => {
            const va = get(compareRows[0]);
            const vb = get(compareRows[1]);
            return (
              <div className="compare-grid" key={label}>
                <span className="dim">{label}</span>
                <span className="mono">{va}</span>
                <span className="mono">{vb}</span>
              </div>
            );
          })}
        </div>
      )}

      {/* 表格 */}
      <div className="card pad" style={{ padding: 0 }}>
        <div className="table-wrap" style={{ maxHeight: "52vh", overflowY: "auto" }}>
          <table className="table">
            <thead>
              <tr>
                <th style={{ width: 34 }}></th>
                <th>时间</th>
                <th>下载 ↓</th>
                <th>上传 ↑</th>
                <th>延迟</th>
                <th>抖动</th>
                <th>丢包</th>
                <th>节点</th>
                <th>网络</th>
                <th>状态</th>
                <th>操作</th>
              </tr>
            </thead>
            <tbody>
              {rows.map((r) => (
                <tr key={r.id} className={selected.includes(r.id) ? "selected" : ""}>
                  <td>
                    <input
                      type="checkbox"
                      checked={selected.includes(r.id)}
                      onChange={() => toggleSel(r.id)}
                      aria-label={`选择记录 ${r.id}`}
                    />
                  </td>
                  <td className="num">{r.created_at}</td>
                  <td className="num" style={{ color: "var(--cyan)", fontWeight: 700 }}>{r.download_mbps.toFixed(2)}</td>
                  <td className="num" style={{ color: "var(--purple)" }}>{r.upload_mbps?.toFixed(2) ?? "—"}</td>
                  <td className="num">{r.idle_ping_ms.toFixed(1)}</td>
                  <td className="num">{r.jitter_ms.toFixed(1)}</td>
                  <td className="num">{r.packet_loss_percent != null ? `${r.packet_loss_percent.toFixed(1)}%` : "—"}</td>
                  <td>{r.server_name || "—"}</td>
                  <td>{r.network_type}</td>
                  <td><span className={`chip ${STATUS_CHIP[r.status]?.cls ?? ""}`}>{STATUS_CHIP[r.status]?.text ?? r.status}</span></td>
                  <td>
                    <div className="row gap4">
                      <button className="btn ghost sm" title="详情回放" onClick={() => openDetail(r.id)}><Icon name="eye" size={13} /></button>
                      <button className="btn ghost sm" title="生成报告" onClick={() => doReport(r.id)}><Icon name="export" size={13} /></button>
                      <button className="btn ghost sm" title="删除" onClick={() => doDelete(r.id)}><Icon name="trash" size={13} /></button>
                    </div>
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
          {rows.length === 0 && (
            <div className="empty">
              <Icon name="history" size={36} />
              <span>{loading ? "加载中…" : invalidRange ? "请先调整日期范围" : loadError ? "历史暂时无法读取，请重试加载" : from || to ? "这个日期范围没有记录，可调整日期或选择全部日期" : "暂无历史记录，完成一次测速后会自动保存"}</span>
            </div>
          )}
        </div>
      </div>

      {confirmClear && (
        <ConfirmModal
          title="清空历史记录"
          text={`将永久删除全部 ${globalTotal ?? rows.length} 条测速记录(含失败/取消的记录),该操作不可恢复。导出的文件不受影响。`}
          danger
          onOk={async () => {
            try {
              await api.historyClear();
              setSelected([]);
              toast("success", "历史记录已清空");
              load();
            } catch (e) {
              toast("error", String(e));
            }
          }}
          onClose={() => { setConfirmClear(false); setGlobalTotal(null); }}
        />
      )}

      {detail && <DetailModal detail={detail} onClose={() => setDetail(null)} />}
    </div>
  );
}

/** 历史详情回放:完整样本曲线 + 全部指标(数据来自测试时保存的 detail JSON) */
function DetailModal({ detail, onClose }: { detail: TestResult; onClose: () => void }) {
  // series 引用稳定:仅当详情数据更新时重建
  const seriesDown = useMemo<Series[]>(
    () => [{ name: "下载", color: "#22d3ee", data: detail.down_samples, fill: true }],
    [detail],
  );
  const seriesUp = useMemo<Series[]>(
    () => [{ name: "上传", color: "#a78bfa", data: detail.up_samples, fill: true }],
    [detail],
  );
  const seriesLat = useMemo<Series[]>(
    () => [{ name: "延迟", color: "#7dd3fc", data: latencyPoints(detail.latency_samples) }],
    [detail],
  );
  return (
    <Modal title={`测速详情回放 · ${detail.started_at}`} onClose={onClose} width={860}>
      <div className="row gap8 wrap mb12">
        <span className={`chip ${STATUS_CHIP[detail.status]?.cls ?? ""}`}>{STATUS_CHIP[detail.status]?.text ?? detail.status}</span>
        <span className="chip">耗时 {(detail.duration_ms / 1000).toFixed(1)}s</span>
        {detail.server && <span className="chip cyan">{detail.server.name}</span>}
        <span className="chip">{detail.network_type}</span>
        {detail.isp && <span className="chip">{detail.isp}</span>}
      </div>

      <div className="detail-metrics mb12">
        <div><span className="faint">下载</span><b className="mono" style={{ color: "var(--cyan)" }}>{detail.download_mbps.toFixed(2)}</b><span className="faint">Mbps · 峰值 {detail.download_peak_mbps.toFixed(1)}</span></div>
        <div><span className="faint">上传</span><b className="mono" style={{ color: "var(--purple)" }}>{detail.upload_mbps?.toFixed(2) ?? "—"}</b><span className="faint">{detail.upload_peak_mbps ? `Mbps · 峰值 ${detail.upload_peak_mbps.toFixed(1)}` : ""}</span></div>
        <div><span className="faint">延迟</span><b className="mono">{detail.idle_ping_ms.toFixed(1)}</b><span className="faint">ms · 抖动 {detail.jitter_ms.toFixed(1)} ms</span></div>
        <div><span className="faint">丢包</span><b className="mono">{detail.packet_loss_percent != null ? `${detail.packet_loss_percent.toFixed(1)}%` : "未知"}</b><span className="faint">HTTP 失败 {detail.http_failures} 次</span></div>
      </div>

      <div className="card pad mb12" style={{ background: "rgba(5,8,16,.4)" }}>
        <div className="chart-title">下载速度过程</div>
        <LineChart series={seriesDown} yUnit="Mbps" avgLine peakMark height={150} />
      </div>
      <div className="card pad mb12" style={{ background: "rgba(5,8,16,.4)" }}>
        <div className="chart-title">上传速度过程</div>
        <LineChart series={seriesUp} yUnit="Mbps" avgLine peakMark height={150} />
      </div>
      <div className="card pad mb12" style={{ background: "rgba(5,8,16,.4)" }}>
        <div className="chart-title">延迟时间线</div>
        <LineChart series={seriesLat} yUnit="ms" height={150} emptyText="无延迟样本" />
      </div>

      {detail.status === "completed" && detail.quality && (
        <>
          <div className="chart-title">质量评级:{detail.quality.rating}({detail.quality.score}/100)</div>
          <ul className="quality-notes mb12">
            {detail.quality.notes.map((n, i) => <li key={i}>{n}</li>)}
          </ul>
        </>
      )}
      {detail.conditions.length > 0 && (
        <div className="faint" style={{ fontSize: 11.5 }}>测试条件:{detail.conditions.join(";")}</div>
      )}
      {detail.error && <div className="test-error">{detail.error}</div>}
    </Modal>
  );
}

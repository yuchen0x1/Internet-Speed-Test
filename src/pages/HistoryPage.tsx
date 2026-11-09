// 测速历史:筛选/排序/对比/趋势/导出/报告/删除
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { api } from "../api";
import { useApp } from "../store/app";
import { useTest } from "../store/test";
import { useHistoryUi } from "../store/ui";
import { isCancelError, latencyPoints } from "../lib/format";
import { LineChart, type Series } from "../components/LineChart";
import { HistoryHeatmap } from "../components/HistoryHeatmap";
import { ConfirmModal, Modal } from "../components/ui";
import { Icon } from "../components/Icon";
import type { HistoryRow, TestResult } from "../types";

const STATUS_CHIP: Record<string, { cls: string; text: string }> = {
  completed: { cls: "green", text: "完成" },
  cancelled: { cls: "amber", text: "已取消" },
  failed: { cls: "red", text: "失败" },
};

export function HistoryPage() {
  const toast = useApp((s) => s.toast);
  const lastResult = useTest((s) => s.result);
  // 筛选与选择提升到 store:切页回来不丢(见 store/ui.ts)
  const { from, to, desc, selected, setFrom, setTo, toggleDesc, setSelected, toggleSel } = useHistoryUi();
  const [rows, setRows] = useState<HistoryRow[]>([]);
  const [loading, setLoading] = useState(false);
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

  const load = useCallback(async () => {
    const seq = ++loadSeq.current;
    setLoading(true);
    try {
      const list = await api.historyList({ from: from || undefined, to: to || undefined, desc, limit: 500 });
      if (seq !== loadSeq.current) return; // 过期响应:丢弃,不覆盖新筛选结果
      setRows(list);
    } catch (e) {
      if (seq === loadSeq.current) toast("error", `历史加载失败:${String(e)}`);
    } finally {
      if (seq === loadSeq.current) setLoading(false);
    }
  }, [from, to, desc, toast]);

  useEffect(() => {
    load();
  }, [load, lastResult?.id]);

  const stats = useMemo(() => {
    const done = rows.filter((r) => r.status === "completed");
    const n = done.length;
    const avg = (f: (r: HistoryRow) => number) => (n ? done.reduce((a, r) => a + f(r), 0) / n : null);
    const best = done.reduce<HistoryRow | null>(
      (b, r) => (!b || r.download_mbps > b.download_mbps ? r : b), null);
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
    return { n, d: avg((r) => r.download_mbps), u: avg((r) => r.upload_mbps ?? 0),
             p: avg((r) => r.idle_ping_ms), best, daily };
  }, [rows]);

  const trendData = useMemo(() => {
    const asc = [...rows].filter((r) => r.status === "completed").reverse().slice(-40);
    const down: Series = {
      name: "下载", color: "#22d3ee",
      data: asc.map((r, i) => [i, r.download_mbps] as [number, number]),
      fill: true,
    };
    const up: Series = {
      name: "上传", color: "#a78bfa",
      data: asc.map((r, i) => [i, r.upload_mbps ?? 0] as [number, number]),
    };
    return { down, up, n: asc.length };
  }, [rows]);

  // series 数组引用稳定,避免 LineChart 无谓重绘
  const trendSeries = useMemo<Series[]>(() => [trendData.down, trendData.up], [trendData]);

  const compareRows = rows.filter((r) => selected.includes(r.id));

  const doExport = async (format: "csv" | "json") => {
    try {
      const path = await api.historyExport(format);
      toast("success", `已导出:${path}`);
    } catch (e) {
      if (!isCancelError(e)) toast("error", `导出失败:${String(e)}`);
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
          <input type="date" className="text-input" value={from} onChange={(e) => setFrom(e.target.value)} aria-label="起始日期" />
          <span className="faint">至</span>
          <input type="date" className="text-input" value={to} onChange={(e) => setTo(e.target.value)} aria-label="结束日期" />
          <button className="btn sm" onClick={toggleDesc}>
            <Icon name="clock" size={13} /> {desc ? "最新在前" : "最早在前"}
          </button>
          <button className="btn sm" onClick={() => doExport("csv")}><Icon name="export" size={13} /> CSV</button>
          <button className="btn sm" onClick={() => doExport("json")}><Icon name="export" size={13} /> JSON</button>
          <button className="btn sm danger" onClick={() => setConfirmClear(true)}><Icon name="trash" size={13} /> 清空</button>
        </div>
      </div>

      {/* 统计卡行 */}
      {stats.n > 0 && (
        <div className="stat-cards-row mb16">
          <div className="card pad"><div className="stat-head"><Icon name="history" size={15} /> 总测试</div>
            <div className="stat-value mono" style={{ fontSize: 24 }}>{stats.n}</div><div className="stat-unit">次完成</div></div>
          <div className="card pad"><div className="stat-head"><Icon name="download" size={15} /> 平均下载</div>
            <div className="stat-value mono" style={{ fontSize: 24, color: "var(--cyan)" }}>{stats.d?.toFixed(1) ?? "—"}</div><div className="stat-unit">Mbps</div></div>
          <div className="card pad"><div className="stat-head"><Icon name="upload" size={15} /> 平均上传</div>
            <div className="stat-value mono" style={{ fontSize: 24, color: "var(--purple)" }}>{stats.u?.toFixed(1) ?? "—"}</div><div className="stat-unit">Mbps</div></div>
          <div className="card pad"><div className="stat-head"><Icon name="target" size={15} /> 平均延迟</div>
            <div className="stat-value mono" style={{ fontSize: 24 }}>{stats.p?.toFixed(1) ?? "—"}</div><div className="stat-unit">ms</div></div>
          <div className="card pad"><div className="stat-head"><Icon name="zap" size={15} /> 最佳下载</div>
            <div className="stat-value mono" style={{ fontSize: 24, color: "var(--green)" }}>{stats.best?.download_mbps.toFixed(1) ?? "—"}</div>
            <div className="stat-unit">{stats.best ? stats.best.created_at.slice(5, 10) : ""}</div></div>
        </div>
      )}
      {stats.daily.length > 1 && (
        <div className="card pad mb16">
          <div className="chart-title">日均下载对比(最近 {stats.daily.length} 个测试日,悬停查看)</div>
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
          <div className="chart-title">历史热力图(按天平均下载速度 · 最近 14 周)</div>
          <HistoryHeatmap rows={rows} weeks={14} />
        </div>
      )}

      {/* 趋势图 */}
      {trendData.n >= 2 && (
        <div className="card pad mb16">
          <div className="chart-title">历史速度趋势(最近 {trendData.n} 次成功测试,悬停查看)</div>
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
              <span>{loading ? "加载中…" : "暂无历史记录 —— 完成一次测速后会自动保存"}</span>
            </div>
          )}
        </div>
      </div>

      {confirmClear && (
        <ConfirmModal
          title="清空历史记录"
          text={`将永久删除全部 ${rows.length} 条测速记录(含失败/取消的记录),该操作不可恢复。导出的文件不受影响。`}
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
          onClose={() => setConfirmClear(false)}
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

      {detail.quality && (
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

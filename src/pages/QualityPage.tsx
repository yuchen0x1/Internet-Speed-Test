// 网络质量分析页:可解释评级 + 负载延迟对比 + 抖动/丢包详情
import { useMemo } from "react";
import { useApp } from "../store/app";
import { useTest } from "../store/test";
import { latencyPoints } from "../lib/format";
import { stabilityText, stabilityTone } from "../lib/quality";
import { LineChart, type Series } from "../components/LineChart";
import { RadarChart, type RadarMetric } from "../components/RadarChart";
import { Icon } from "../components/Icon";

const RATING_COLORS: Record<string, string> = {
  优秀: "var(--green)",
  良好: "var(--cyan)",
  一般: "var(--amber)",
  较差: "var(--red)",
};

function Metric(props: { label: string; value: string; unit?: string; hint?: string; tone?: "good" | "warn" | "bad" | "neutral" }) {
  const toneColor = { good: "var(--green)", warn: "var(--amber)", bad: "var(--red)", neutral: "var(--text)" }[props.tone ?? "neutral"];
  return (
    <div className="card pad hoverable">
      <div className="stat-head">{props.label}</div>
      <div className="stat-value mono" style={{ color: toneColor, fontSize: 26 }}>
        {props.value}
        {props.unit && <span style={{ fontSize: 13, color: "var(--text-dim)", marginLeft: 6 }}>{props.unit}</span>}
      </div>
      {props.hint && <div className="stat-unit">{props.hint}</div>}
    </div>
  );
}

export function QualityPage() {
  const setPage = useApp((s) => s.setPage);
  const result = useTest((s) => s.result);

  // series 引用稳定:仅当结果更新时重建,避免 LineChart 重绘(hooks 须在条件返回前调用)
  const latencySeries = useMemo<Series[]>(
    () => (result ? [{ name: "延迟", color: "#7dd3fc", data: latencyPoints(result.latency_samples), fill: true }] : []),
    [result],
  );
  const downSeries = useMemo<Series[]>(
    () => (result ? [{ name: "下载速度", color: "#22d3ee", data: result.down_samples, fill: true }] : []),
    [result],
  );
  const upSeries = useMemo<Series[]>(
    () => (result ? [{ name: "上传速度", color: "#a78bfa", data: result.up_samples, fill: true }] : []),
    [result],
  );

  if (!result || result.status !== "completed") {
    return (
      <div>
        <Head />
        <div className="card pad">
          <div className="empty">
            <Icon name="wave" size={40} />
            <span>{result?.status === "cancelled" ? "本次测速已取消，未生成完整质量评级；已采集的样本可在历史详情查看。" : "还没有可分析的质量数据 —— 请先完成一次测速"}</span>
            <button className="btn primary mt8" onClick={() => setPage("speedtest")}>
              <Icon name="play" size={14} /> 前往测速
            </button>
          </div>
        </div>
      </div>
    );
  }

  const q = result.quality;
  const idle = result.idle_ping_ms;
  const dlRatio = result.loaded_down_ms && idle > 0 ? result.loaded_down_ms / idle : null;
  const ulRatio = result.loaded_up_ms && idle > 0 ? result.loaded_up_ms / idle : null;

  const bars = [
    { label: "空闲", v: idle, color: "var(--ice)" },
    { label: "下载负载", v: result.loaded_down_ms, color: "var(--cyan)" },
    { label: "上传负载", v: result.loaded_up_ms, color: "var(--purple)" },
  ];
  const maxBar = Math.max(...bars.map((b) => b.v ?? 0), 1);

  return (
    <div>
      <Head />

      <div className="quality-grid">
        {/* 评级卡 */}
        <div className="card pad" style={{ display: "flex", flexDirection: "column", alignItems: "center", justifyContent: "center", gap: 8 }}>
          <div
            className="rating-ring"
            style={{ borderColor: RATING_COLORS[q?.rating ?? "一般"], boxShadow: `0 0 24px ${RATING_COLORS[q?.rating ?? "一般"]}33` }}
          >
            <span className="rating-score">{q?.score ?? "--"}</span>
            <span className="rating-total">/100</span>
          </div>
          <div className="rating-label" style={{ color: RATING_COLORS[q?.rating ?? "一般"] }}>{q?.rating ?? "未知"}</div>
          <div className="faint" style={{ fontSize: 11.5 }}>
            综合延迟 / 抖动 / 负载 / 丢包评估
          </div>
        </div>

        {/* 指标网格 */}
        <div className="quality-metrics">
          <Metric label="空闲延迟" value={idle.toFixed(1)} unit="ms" tone={idle < 40 ? "good" : idle < 90 ? "warn" : "bad"} hint={`最小 ${result.ping_min_ms.toFixed(1)} / 最大 ${result.ping_max_ms.toFixed(1)}`} />
          <Metric label="延迟抖动" value={result.jitter_ms.toFixed(1)} unit="ms" tone={result.jitter_ms < 5 ? "good" : result.jitter_ms < 15 ? "warn" : "bad"} hint="相邻延迟样本波动" />
          <Metric
            label="下载负载延迟"
            value={result.loaded_down_ms ? result.loaded_down_ms.toFixed(0) : "未测"}
            unit={result.loaded_down_ms ? "ms" : ""}
            tone={!dlRatio ? "neutral" : dlRatio < 1.5 ? "good" : dlRatio < 2.5 ? "warn" : "bad"}
            hint={dlRatio ? `为空闲的 ${dlRatio.toFixed(2)} 倍` : "上传/下载阶段未获得"}
          />
          <Metric
            label="上传负载延迟"
            value={result.loaded_up_ms ? result.loaded_up_ms.toFixed(0) : "未测"}
            unit={result.loaded_up_ms ? "ms" : ""}
            tone={!ulRatio ? "neutral" : ulRatio < 1.5 ? "good" : ulRatio < 2.5 ? "warn" : "bad"}
            hint={ulRatio ? `为空闲的 ${ulRatio.toFixed(2)} 倍` : "上传阶段未获得"}
          />
          <Metric
            label="丢包率(ICMP)"
            value={result.packet_loss_percent != null ? result.packet_loss_percent.toFixed(1) : "未知"}
            unit={result.packet_loss_percent != null ? "%" : ""}
            tone={result.packet_loss_percent == null ? "neutral" : result.packet_loss_percent <= 0 ? "good" : result.packet_loss_percent < 3 ? "warn" : "bad"}
            hint={result.packet_loss_percent == null ? "目标禁 ping 或系统限制" : "真实网络层丢包"}
          />
          <Metric
            label="网络稳定性"
            value={stabilityText(result.jitter_ms, dlRatio, result.packet_loss_percent)}
            tone={stabilityTone(result.jitter_ms, dlRatio, result.packet_loss_percent)}
            hint={`HTTP 失败 ${result.http_failures} 次(与丢包分开统计)`}
          />
        </div>
      </div>

      {/* 五维雷达 + 负载延迟对比 */}
      <div className="quality-grid2 mt16">
        <div className="card pad" style={{ display: "flex", flexDirection: "column", alignItems: "center" }}>
          <div className="chart-title" style={{ alignSelf: "flex-start" }}>网络五维雷达(越外圈越好)</div>
          {(() => {
            const dl = result.download_mbps;
            const ul = result.upload_mbps ?? 0;
            const metrics: RadarMetric[] = [
              { label: "延迟", value: idle, max: 200, lowerBetter: true, display: `${idle.toFixed(0)} ms` },
              { label: "抖动", value: result.jitter_ms, max: 30, lowerBetter: true, display: `${result.jitter_ms.toFixed(1)} ms` },
              { label: "丢包", value: result.packet_loss_percent, max: 5, lowerBetter: true, display: result.packet_loss_percent != null ? `${result.packet_loss_percent.toFixed(1)}%` : "未知" },
              { label: "下载", value: dl, max: 500, display: `${dl.toFixed(0)} Mbps` },
              { label: "上传", value: result.upload_mbps, max: 200, display: result.upload_mbps != null ? `${ul.toFixed(0)} Mbps` : "未测" },
            ];
            return <RadarChart metrics={metrics} size={250} />;
          })()}
        </div>
        <div className="card pad">
          <div className="chart-title">延迟对比:空闲 vs 负载(Bufferbloat)</div>
          <div style={{ display: "flex", flexDirection: "column", gap: 12, padding: "10px 4px" }}>
            {bars.map((b) => (
              <div key={b.label}>
                <div className="row" style={{ justifyContent: "space-between", fontSize: 12, marginBottom: 5 }}>
                  <span className="dim">{b.label}</span>
                  <b className="mono">{b.v != null ? `${b.v.toFixed(1)} ms` : "未测"}</b>
                </div>
                <div className="progress">
                  <div style={{ width: `${((b.v ?? 0) / maxBar) * 100}%`, background: b.color, boxShadow: `0 0 10px ${b.color}` }} />
                </div>
              </div>
            ))}
          </div>
          <div className="faint" style={{ fontSize: 11.5, marginTop: 8 }}>
            负载延迟显著高于空闲延迟(Bufferbloat)意味着带宽满载时,游戏/语音会明显卡顿。
          </div>
        </div>

        <div className="card pad">
          <div className="chart-title">质量分析说明(由本次真实数据生成)</div>
          <ul className="quality-notes">
            {(q?.notes ?? ["暂无"]).map((n, i) => (
              <li key={i}>{n}</li>
            ))}
          </ul>
          {result.conditions.length > 0 && (
            <div className="faint" style={{ fontSize: 11.5, marginTop: 10 }}>
              测试条件:{result.conditions.join(";")}
            </div>
          )}
        </div>
      </div>

      {/* 曲线 */}
      <div className="charts-grid mt16">
        <div className="card pad">
          <div className="chart-title">延迟时间线</div>
          <LineChart series={latencySeries} yUnit="ms" height={170} />
        </div>
        <div className="card pad">
          <div className="chart-title">下载速度过程</div>
          <LineChart series={downSeries} yUnit="Mbps" avgLine height={170} />
        </div>
        <div className="card pad">
          <div className="chart-title">上传速度过程</div>
          <LineChart series={upSeries} yUnit="Mbps" avgLine height={170} />
        </div>
      </div>
    </div>
  );
}

function Head() {
  return (
    <div className="page-head">
      <div>
        <h1 className="page-title">
          <span className="icon-wrap"><Icon name="wave" size={22} /></span>
          网络质量
        </h1>
        <div className="page-sub">基于最近一次真实测速结果的深度分析</div>
      </div>
    </div>
  );
}

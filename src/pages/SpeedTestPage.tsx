// 网络测速主页:仪表盘 + 三大指标卡 + 实时曲线 + 阶段时间线 + 控制区
import React, { useEffect, useMemo, useState } from "react";
import { useShallow } from "zustand/react/shallow";
import { api } from "../api";
import { useApp } from "../store/app";
import { fmtSpeedMbps, latencyPoints } from "../lib/format";
import { useMotionEnabled } from "../lib/motion";
import { useTest } from "../store/test";
import { Gauge } from "../components/Gauge";
import { LineChart, type Series } from "../components/LineChart";
import { GlowCard } from "../components/GlowCard";
import { Icon } from "../components/Icon";
import { SpeedTestServices } from "../components/SpeedTestServices";

const STEPS: { key: string; label: string }[] = [
  { key: "preparing", label: "准备" },
  { key: "latency", label: "延迟" },
  { key: "download", label: "下载" },
  { key: "upload", label: "上传" },
  { key: "quality", label: "质量" },
  { key: "finished", label: "完成" },
];

export function SpeedTestPage() {
  const toast = useApp((s) => s.toast);
  const settings = useApp((s) => s.settings);
  // 只订阅本页用到的字段(progress/bound 等无关字段变化不触发重渲染;shallow 避免对象选择器的引用抖动)
  const t = useTest(
    useShallow((s) => ({
      phase: s.phase,
      message: s.message,
      running: s.running,
      elapsedMs: s.elapsedMs,
      node: s.node,
      liveDown: s.liveDown,
      liveUp: s.liveUp,
      downSamples: s.downSamples,
      upSamples: s.upSamples,
      latencySamples: s.latencySamples,
      loadedDown: s.loadedDown,
      loadedUp: s.loadedUp,
      downloadedMb: s.downloadedMb,
      uploadedMb: s.uploadedMb,
      result: s.result,
      error: s.error,
    })),
  );
  const [motionTick, setMotionTick] = useState(0);
  const [openingService, setOpeningService] = useState<string | null>(null);
  const motionOn = useMotionEnabled();

  // 动效关闭时不起 120ms 刷新循环(过程舱静态展示,数据仍由测速事件驱动)
  useEffect(() => {
    if (!t.running || !motionOn) return;
    const id = window.setInterval(() => setMotionTick((v) => v + 1), 120);
    return () => window.clearInterval(id);
  }, [t.running, motionOn]);

  const openService = async (id: string, name: string) => {
    setOpeningService(id);
    try {
      await api.openSpeedtestService(id);
      toast("info", `已在系统浏览器打开 ${name}；第三方站点结果不会写入本机测速历史`);
    } catch (e) {
      toast("error", String(e));
    } finally {
      setOpeningService(null);
    }
  };

  const gaugeData = useMemo(() => {
    switch (t.phase) {
      case "latency": {
        const last = t.latencySamples[t.latencySamples.length - 1];
        return { value: last?.[1] ?? 0, label: "延迟测试中", unit: "ms" };
      }
      case "download":
        return { value: t.liveDown, label: "下载测速中", unit: "Mbps" };
      case "upload":
        return { value: t.liveUp, label: "上传测速中", unit: "Mbps" };
      case "quality":
        return { value: t.liveDown, label: "质量分析中", unit: "Mbps" };
      case "finished":
        return {
          value: t.result?.download_mbps ?? 0,
          label: t.error || t.result?.status === "failed" ? "测速失败" : t.result?.status === "cancelled" || t.message === "已取消" ? "已取消" : "测速完成",
          unit: "Mbps",
        };
      case "preparing":
        return { value: 0, label: "准备中", unit: "" };
      default:
        return { value: 0, label: "准备就绪", unit: "Mbps" };
    }
  }, [t.phase, t.liveDown, t.liveUp, t.latencySamples, t.result, t.error, t.message]);

  const gaugeMax = useMemo(() => {
    const peak = Math.max(
      t.liveDown, t.liveUp,
      t.result?.download_mbps ?? 0, t.result?.upload_mbps ?? 0,
      ...t.downSamples.slice(-30).map((s) => s[1]),
      ...t.upSamples.slice(-30).map((s) => s[1]),
    );
    if (t.phase === "latency") return 200; // 延迟 ms 量程(动态无意义,固定标尺)
    if (peak <= 0) return 100;
    return Math.max(10, Math.ceil(peak * 1.25));
  }, [t.liveDown, t.liveUp, t.downSamples, t.upSamples, t.result, t.phase]);

  const gaugeProgress = useMemo(() => {
    if (t.phase === "latency") {
      return Math.min(1, t.latencySamples.length / 10);
    }
    if (t.phase === "download" || t.phase === "upload") {
      // 以测试设置时长估算(20 样本/s... 200ms 粒度 → 5/s)
      const dur = (settings?.test_duration_secs ?? 10) * 5;
      const samples = t.phase === "download" ? t.downSamples : t.upSamples;
      return Math.min(1, samples.length / dur);
    }
    return undefined;
  }, [t.phase, t.latencySamples.length, t.downSamples.length, t.upSamples.length, settings?.test_duration_secs]);

  const start = async () => {
    try {
      await api.speedtestStart();
      toast("info", "测速已启动");
    } catch (e) {
      toast("error", String(e));
    }
  };
  const stop = async () => {
    try {
      await api.speedtestStop();
      toast("warn", "正在取消测速任务");
    } catch (e) {
      toast("error", String(e));
    }
  };

  const stepIdx = STEPS.findIndex((s) => s.key === t.phase);
  const dl = fmtSpeedMbps(t.result?.download_mbps ?? t.liveDown);
  const ul = fmtSpeedMbps(t.result?.upload_mbps ?? t.liveUp);
  // series 引用稳定:仅当对应样本数组更新时重建,避免 LineChart 无谓重绘
  const downSeries = useMemo<Series[]>(
    () => [{ name: "下载速度", color: "#22d3ee", data: t.downSamples, fill: true }],
    [t.downSamples],
  );
  const upSeries = useMemo<Series[]>(
    () => [{ name: "上传速度", color: "#a78bfa", data: t.upSamples, fill: true }],
    [t.upSamples],
  );
  const latSeries = useMemo<Series[]>(
    () => [{ name: "延迟", color: "#7dd3fc", data: latencyPoints(t.latencySamples) }],
    [t.latencySamples],
  );
  const latAnomaly = useMemo(() => {
    if (t.latencySamples.length < 4) return undefined;
    const sorted = [...t.latencySamples.map((s) => s[1])].sort((a, b) => a - b);
    const p90 = sorted[Math.floor(sorted.length * 0.9)];
    return p90 * 1.8;
  }, [t.latencySamples]);

  return (
    <div>
      <div className="page-head">
        <div>
          <h1 className="page-title">
            <span className="icon-wrap"><Icon name="gauge" size={22} /></span>
            网络测速
          </h1>
          <div className="page-sub">真实多流测速引擎 · 严格区分 HTTP 失败与网络丢包</div>
        </div>
        <div className="row gap8 wrap">
          <button className="btn primary" onClick={start} disabled={t.running}>
            <Icon name="play" size={15} />
            {t.result && !t.running ? "重新测速" : "开始测速"}
          </button>
          <button className="btn danger" onClick={stop} disabled={!t.running}>
            <Icon name="stop" size={15} />
            停止
          </button>
        </div>
      </div>

      {/* 阶段时间线 */}
      <div className="phase-track card pad mb16">
        {STEPS.map((s, i) => {
          const incomplete = t.phase === "finished" && t.result?.status !== "completed";
          const state =
            incomplete ? "todo" : t.phase === s.key ? "active" : stepIdx > i ? "done" : "todo";
          return (
            <React.Fragment key={s.key}>
              {i > 0 && <div className={`phase-line ${state === "done" || (state === "active" && stepIdx > i) ? "lit" : ""}`} />}
              <div className={`phase-node ${state}`}>
                <span className="phase-dot" />
                {s.label}
              </div>
            </React.Fragment>
          );
        })}
        <div className="grow" />
        <div className="phase-info">
          {t.running && (
            <>
              <span className="chip cyan">{t.message}</span>
              <span className="chip">耗时 {(t.elapsedMs / 1000).toFixed(1)}s</span>
            </>
          )}
        </div>
      </div>

      <div className="speedtest-grid">
        {/* 仪表盘(测速中套用氛围光效) */}
        <GlowCard
          active={t.running}
          message={t.message}
          elapsedSec={(t.elapsedMs / 1000).toFixed(1)}
          className="gauge-card"
        >
          <Gauge
            phase={t.error ? "failed" : t.phase}
            running={t.running}
            value={gaugeData.value}
            label={gaugeData.label}
            unit={gaugeData.unit}
            max={gaugeMax}
            progress={gaugeProgress}
            error={t.error}
            sub={
              t.node
                ? `${t.node.name}${t.result?.public_ip ? ` · IP ${t.result.public_ip}` : ""}`
                : settings?.node_mode === "manual"
                  ? "手动节点(未获取)"
                  : "自动选择最佳节点"
            }
          />
          {t.running && t.phase !== "latency" && (
            <div className="progress mt12" style={{ margin: "14px 18px 0" }}>
              <div style={{ width: `${(gaugeProgress ?? 0) * 100}%` }} />
            </div>
          )}
          {t.error && <div className="test-error">{t.error}</div>}
          {t.result?.history_save_error && <div className="test-error" role="alert">{t.result.history_save_error}。本次结果尚未保存。</div>}
          {!t.running && !t.error && t.result?.status === "completed" && (
            <div className="row gap8" style={{ justifyContent: "center", marginTop: 12 }}>
              <button
                className="btn ghost sm"
                title="复制结果文本"
                onClick={async () => {
                  try {
                    const text = await api.copyResultText(t.result!);
                    await navigator.clipboard.writeText(text);
                    toast("success", "结果已复制到剪贴板");
                  } catch (e) {
                    toast("error", `复制失败:${String(e)}`);
                  }
                }}
              >
                <Icon name="copy" size={13} /> 复制结果
              </button>
              <span className="chip green"><Icon name="check" size={12} /> 测试完成 · {t.result.duration_ms / 1000 < 1 ? "" : `${(t.result.duration_ms / 1000).toFixed(1)}s`}</span>
              {t.result.server && <span className="chip">节点 {t.result.server.name}</span>}
              {t.result.network_type && <span className="chip">{t.result.network_type}</span>}
            </div>
          )}
        </GlowCard>

        {/* 三大指标卡 */}
        <div className="stat-stack">
          <div className="card pad hoverable stat-card">
            <div className="stat-head"><Icon name="download" size={17} /> 下载速度</div>
            <div className="stat-value mono">{t.running && t.phase === "download" ? dl.value : t.result?.download_mbps ? dl.value : t.running ? "--" : "--"}</div>
            <div className="stat-unit">{t.running && t.phase === "download" ? dl.unit : "Mbps"}{t.result ? ` · 峰值 ${t.result.download_peak_mbps.toFixed(1)}` : ""}</div>
          </div>
          <div className="card pad hoverable stat-card purple-accent">
            <div className="stat-head"><Icon name="upload" size={17} /> 上传速度</div>
            <div className="stat-value mono">{t.result?.upload_mbps != null ? ul.value : t.running && t.phase === "upload" ? ul.value : "--"}</div>
            <div className="stat-unit">{t.result?.upload_peak_mbps ? `峰值 ${t.result.upload_peak_mbps.toFixed(1)} Mbps` : ul.unit}</div>
          </div>
          <div className="card pad hoverable stat-card">
            <div className="stat-head"><Icon name="target" size={17} /> 网络延迟</div>
            <div className="stat-value mono">{t.result?.idle_ping_ms ? t.result.idle_ping_ms.toFixed(1) : t.latencySamples.length ? t.latencySamples[t.latencySamples.length - 1][1].toFixed(1) : "--"}</div>
            <div className="stat-unit">ms{t.result ? ` · 抖动 ${t.result.jitter_ms.toFixed(1)} ms` : ""}</div>
          </div>
        </div>
      </div>

      {/* 信息条 */}
      <div className="row gap8 wrap mt16 info-strip">
        <span className="chip"><Icon name="server" size={12} /> {t.node?.name ?? (settings?.node_mode === "auto" ? "自动节点优选" : "手动节点")}</span>
        {settings?.show_public_ip ? (
          <span className="chip"><Icon name="globe" size={12} /> {t.result?.public_ip ?? "公网 IP 获取中/未获取"}{t.result?.isp ? ` · ${t.result.isp}` : ""}</span>
        ) : (
          <span className="chip"><Icon name="eye" size={12} /> 公网 IP 已隐藏(设置中可开启)</span>
        )}
        <span className="chip"><Icon name="cpu" size={12} /> {settings?.down_concurrency ?? 6} ↓ / {settings?.up_concurrency ?? 4} ↑ 连接</span>
        <span className="chip"><Icon name="clock" size={12} /> {settings?.test_duration_secs ?? 10}s × 2 阶段</span>
        {settings?.precision_mode && <span className="chip purple">精度模式</span>}
      </div>

      {/* 测速动态过程舱：所有活动状态均由测速事件与真实采样驱动 */}
      <DataFlowStage
        phase={t.phase}
        running={t.running}
        message={t.message}
        node={t.node?.name ?? "等待节点选择"}
        speed={t.phase === "upload" ? t.liveUp : t.liveDown}
        downloadedMb={t.downloadedMb}
        uploadedMb={t.uploadedMb}
        latency={t.latencySamples.at(-1)?.[1] ?? null}
        loadedLatency={t.phase === "download" ? t.loadedDown.at(-1) : t.loadedUp.at(-1)}
        streams={t.phase === "download" ? settings?.down_concurrency ?? 6 : settings?.up_concurrency ?? 4}
        motionTick={motionTick}
      />

      <SpeedTestServices opening={openingService} onOpen={openService} />

      {/* 实时图表 */}
      <div className="charts-grid mt16">
        <div className="card pad">
          <div className="chart-title">下载速度实时曲线</div>
          <LineChart series={downSeries} yUnit="Mbps" windowSecs={Math.max(12, (settings?.test_duration_secs ?? 10) + 2)} avgLine peakMark height={180} />
        </div>
        <div className="card pad">
          <div className="chart-title">上传速度实时曲线</div>
          <LineChart series={upSeries} yUnit="Mbps" windowSecs={Math.max(12, (settings?.test_duration_secs ?? 10) + 2)} avgLine peakMark height={180} />
        </div>
        <div className="card pad">
          <div className="chart-title">延迟曲线(红点为异常尖峰)</div>
          <LineChart series={latSeries} yUnit="ms" anomalyThreshold={latAnomaly} height={180} emptyText="延迟测试开始后显示" />
        </div>
      </div>
    </div>
  );
}

function DataFlowStage(props: {
  phase: string;
  running: boolean;
  message: string;
  node: string;
  speed: number;
  downloadedMb: number;
  uploadedMb: number;
  latency: number | null;
  loadedLatency: number | undefined;
  streams: number;
  motionTick: number;
}) {
  const {
    phase, running, message, node, speed, downloadedMb, uploadedMb,
    latency, loadedLatency, streams, motionTick,
  } = props;
  const direction = phase === "upload" ? "upload" : "download";
  const activePhase = phase === "download" || phase === "upload";
  const dataMb = direction === "upload" ? uploadedMb : downloadedMb;
  const activity = Math.min(1, Math.max(0, speed) / 1000);
  const pulse = (motionTick % 100) / 100;
  // 数据包速度由实测速率驱动:越快 → 包速越快(仅表现"进行中",不代表测量值)
  const packetSpeed = activePhase ? Math.max(0.55, 2.6 - activity * 2.0) : 2.6;
  const scanPos = direction === "upload" ? 100 - pulse * 100 : pulse * 100;
  const phaseLabel: Record<string, string> = {
    preparing: "节点发现与握手",
    latency: "往返延迟采样",
    download: "多流接收测试数据",
    upload: "多流发送测试数据",
    quality: "负载与链路质量分析",
    finished: "本轮测试已结束",
    idle: "等待开始测速",
  };
  const events = [
    { time: "01", label: "解析测速节点", done: !!node && node !== "等待节点选择", live: phase === "preparing" },
    { time: "02", label: "建立 HTTPS 测试连接", done: ["latency", "download", "upload", "quality", "finished"].includes(phase), live: phase === "preparing" && !!node && node !== "等待节点选择" },
    { time: "03", label: "采集真实 RTT / 吞吐样本", done: ["download", "upload", "quality", "finished"].includes(phase), live: phase === "latency" || phase === "download" || phase === "upload" },
    { time: "04", label: "质量评分与结果入库", done: phase === "finished", live: phase === "quality" },
  ];

  return (
    <section className={`card pad flow-stage ${running ? "active" : ""} ${activePhase ? direction : ""}`}>
      <div className="flow-head">
        <div>
          <div className="chart-title"><span className={`flow-live-dot ${running ? "on" : ""}`} />实时测速过程</div>
          <div className="faint flow-caption">{message || phaseLabel[phase] || "数据流动来自真实测速采样"}</div>
        </div>
        <span className={`chip ${running ? "cyan" : phase === "finished" ? "green" : ""}`}>{phaseLabel[phase] || "空闲"}</span>
      </div>

      <div className="flow-visual" aria-label="测速数据流动动画">
        <div className="flow-endpoint source">
          <span className="flow-endpoint-icon"><Icon name={direction === "upload" ? "cpu" : "server"} size={19} /></span>
          <b>{direction === "upload" ? "本机" : "测速节点"}</b>
          <small>{direction === "upload" ? "随机数据块" : node}</small>
        </div>
        <div className="flow-track">
          <div className="flow-track-line">
            {running && <div className="flow-scan" style={{ left: `${scanPos}%` }} />}
          </div>
          <div className={`flow-packet packet-a ${running ? "moving" : ""}`} style={{ "--packet-speed": `${packetSpeed}s` } as React.CSSProperties} />
          <div className={`flow-packet packet-b ${running ? "moving" : ""}`} style={{ "--packet-speed": `${packetSpeed}s` } as React.CSSProperties} />
          <div className={`flow-packet packet-c ${running ? "moving" : ""}`} style={{ "--packet-speed": `${packetSpeed}s` } as React.CSSProperties} />
          <div className="flow-streams">
            {Array.from({ length: Math.min(Math.max(streams, 1), 8) }, (_, i) => (
              <i key={i} className={running && activePhase ? "stream-on" : ""} style={{ animationDelay: `${i * 0.1}s` }} />
            ))}
          </div>
          <div className="flow-throughput"><b className="mono">{activePhase ? speed.toFixed(1) : "—"}</b><small>Mbps</small></div>
        </div>
        <div className="flow-endpoint destination">
          <span className="flow-endpoint-icon"><Icon name={direction === "upload" ? "server" : "cpu"} size={19} /></span>
          <b>{direction === "upload" ? "测速节点" : "本机"}</b>
          <small>{direction === "upload" ? node : "接收并计数"}</small>
        </div>
      </div>

      <div className="flow-stats">
        <div><small>方向</small><b className={direction === "upload" ? "purple-text" : "cyan-text"}>{activePhase ? (direction === "upload" ? "上传流 ↑" : "下载流 ↓") : "待命"}</b></div>
        <div><small>并发连接</small><b className="mono">{activePhase ? streams : 0}<em> streams</em></b></div>
        <div><small>本阶段真实数据</small><b className="mono">{activePhase ? dataMb.toFixed(2) : "0.00"}<em> MB</em></b></div>
        <div><small>{activePhase ? "负载 RTT" : "最近 RTT"}</small><b className="mono">{(activePhase ? loadedLatency : latency) != null ? `${(activePhase ? loadedLatency : latency)!.toFixed(1)} ms` : "—"}</b></div>
      </div>

      <div className="flow-event-list">
        {events.map((event) => (
          <div className={`flow-event ${event.done ? "done" : ""} ${event.live ? "live" : ""}`} key={event.time}>
            <span>{event.done ? <Icon name="check" size={12} /> : event.live ? <i /> : event.time}</span>
            <small>{event.label}</small>
          </div>
        ))}
      </div>
    </section>
  );
}

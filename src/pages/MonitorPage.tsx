// 实时网络监控:网卡级真实速率 + 时间窗切换 + 暂停/恢复
import { useEffect, useMemo, useRef, useState } from "react";
import { api, onPingMonTick } from "../api";
import { useApp } from "../store/app";
import { fmtBytes, fmtBytesPerSec } from "../lib/format";
import { useMonitor } from "../store/app";
import { LineChart, type Series } from "../components/LineChart";
import { ActivityBars } from "../components/ActivityBars";
import { PingDots, pingStats, type PingSample } from "../components/PingDots";
import { Icon } from "../components/Icon";

const WINDOWS = [
  { value: 60, label: "1 分钟" },
  { value: 300, label: "5 分钟" },
  { value: 900, label: "15 分钟" },
  { value: 3600, label: "1 小时" },
];

export function MonitorPage() {
  const toast = useApp((s) => s.toast);
  const settings = useApp((s) => s.settings);
  const updateSettings = useApp((s) => s.updateSettings);
  const { buffer, latest, paused, bind, setPaused, backfill } = useMonitor();
  const [win, setWin] = useState(300);
  const [pingHost, setPingHost] = useState("223.5.5.5");
  const [pingSamples, setPingSamples] = useState<PingSample[]>([]);
  const [pingRunning, setPingRunning] = useState(false);
  const [pingBusy, setPingBusy] = useState(false);
  const [pingReady, setPingReady] = useState(false);
  const pingRef = useRef<PingSample[]>([]);

  useEffect(() => {
    bind().catch((e) => toast("error", `监控事件连接失败：${String(e)}`));
  }, [bind]);

  // 连续 Ping 事件流
  useEffect(() => {
    let alive = true;
    let un: (() => void) | null = null;
    onPingMonTick((t) => {
      if (!alive) return;
      pingRef.current = [...pingRef.current, { ts: t.ts_ms, rtt: t.rtt_ms }].slice(-300);
      setPingSamples(pingRef.current);
    }).then((off) => {
      if (!alive) off();
      else un = off;
    });
    // 恢复既有监测状态(如页面切换回来)
    api.pingmonHost().then((h) => {
      if (alive && h) { setPingHost(h); setPingRunning(true); }
    }).catch((e) => { if (alive) toast("error", `监测状态读取失败：${String(e)}`); })
      .finally(() => { if (alive) setPingReady(true); });
    return () => {
      alive = false;
      un?.();
    };
  }, []);

  const pingToggle = async () => {
    if (pingBusy || !pingReady) return;
    setPingBusy(true);
    try {
      if (pingRunning) {
        await api.pingmonStop();
        setPingRunning(false);
        return;
      }
      await api.pingmonStart(pingHost.trim());
      pingRef.current = [];
      setPingSamples([]);
      setPingRunning(true);
    } catch (e) {
      toast("error", String(e));
    } finally {
      setPingBusy(false);
    }
  };

  const pStats = pingStats(pingSamples);

  useEffect(() => {
    backfill(win);
  }, [win, backfill]);

  const series: Series[] = useMemo(() => {
    const cutoff = Date.now() - win * 1000;
    const data = buffer.filter((t) => t.ts_ms >= cutoff);
    const rx: [number, number][] = data.map((t) => [(t.ts_ms / 1000) % 1e6, (t.rx_bps * 8) / 1e6]);
    const tx: [number, number][] = data.map((t) => [(t.ts_ms / 1000) % 1e6, (t.tx_bps * 8) / 1e6]);
    return [
      { name: "接收", color: "#22d3ee", data: rx, fill: true },
      { name: "发送", color: "#a78bfa", data: tx, fill: true },
    ];
  }, [buffer, win]);

  const topIfaces = latest?.ifaces.slice(0, 6) ?? [];

  const toggleMonitor = async (on: boolean) => {
    const saved = await updateSettings({ monitor_enabled: on });
    if (!saved) return;
    toast("info", on ? "后台监控已开启" : "后台监控已关闭(不再采样,零开销)");
  };

  return (
    <div>
      <div className="page-head">
        <div>
          <h1 className="page-title">
            <span className="icon-wrap"><Icon name="activity" size={22} /></span>
            网络监控
          </h1>
          <div className="page-sub">
            网卡层真实流量(包含所有进程)—— 与测速流量相互独立,不混淆
            {paused && " · 已暂停"}
          </div>
        </div>
        <div className="row gap8">
          <button className="btn" onClick={() => setPaused(!paused).catch((e) => toast("error", `监控状态切换失败：${String(e)}`))}>
            <Icon name={paused ? "play" : "pause"} size={14} />
            {paused ? "恢复" : "暂停"}
          </button>
        </div>
      </div>

      {/* 概览卡 */}
      <div className="monitor-cards">
        <div className="card pad hoverable">
          <div className="stat-head"><Icon name="download" size={16} /> 接收速率</div>
          <div className="row" style={{ justifyContent: "space-between", alignItems: "flex-end" }}>
            <div className="stat-value mono" style={{ color: "var(--cyan)" }}>{fmtBytesPerSec(latest?.rx_bps ?? 0)}</div>
            <ActivityBars history={buffer.map((t) => t.rx_bps)} color="var(--cyan)" height={26} bars={8} />
          </div>
          <div className="stat-unit">累计接收 {fmtBytes(latest?.rx_total ?? 0)}(开机以来,全部网卡)</div>
        </div>
        <div className="card pad hoverable">
          <div className="stat-head"><Icon name="upload" size={16} /> 发送速率</div>
          <div className="row" style={{ justifyContent: "space-between", alignItems: "flex-end" }}>
            <div className="stat-value mono" style={{ color: "var(--purple)" }}>{fmtBytesPerSec(latest?.tx_bps ?? 0)}</div>
            <ActivityBars history={buffer.map((t) => t.tx_bps)} color="var(--purple)" height={26} bars={8} />
          </div>
          <div className="stat-unit">累计发送 {fmtBytes(latest?.tx_total ?? 0)}</div>
        </div>
        <div className="card pad hoverable">
          <div className="stat-head"><Icon name="cpu" size={16} /> 后台监控</div>
          <div className="row mt8">
            <button className={`toggle ${settings?.monitor_enabled ? "on" : ""}`} onClick={() => toggleMonitor(!settings?.monitor_enabled)} aria-label="后台监控开关" />
            <span className="dim" style={{ fontSize: 12.5 }}>{settings?.monitor_enabled ? "运行中(1s 采样)" : "已关闭"}</span>
          </div>
          <div className="stat-unit mt8">关闭后停止采样,节省 CPU/电量</div>
        </div>
      </div>

      {/* 曲线 */}
      <div className="card pad mt16">
        <div className="row mb8">
          <div className="chart-title grow" style={{ margin: 0 }}>实时流量曲线</div>
          <div className="segmented">
            {WINDOWS.map((w) => (
              <button key={w.value} className={win === w.value ? "active" : ""} onClick={() => setWin(w.value)}>
                {w.label}
              </button>
            ))}
          </div>
        </div>
        <LineChart series={series} yUnit="Mbps" height={230} windowSecs={win} avgLine emptyText={paused ? "已暂停 —— 点击恢复继续记录" : "正在采集数据…"} />
        <div className="faint mt8" style={{ fontSize: 11.5 }}>
          纵轴为折算带宽(Mbps)。速率由网卡字节计数在 1s 间隔上的差分得出,是整机真实流量。
        </div>
      </div>

      {/* 连续 Ping 监测(gping 式) */}
      <div className="card pad mt16">
        <div className="row mb8">
          <div className="chart-title grow" style={{ margin: 0 }}><Icon name="target" size={15} /> 连续 Ping 监测(1s 真实 ICMP)</div>
          <input className="text-input" style={{ width: 220 }} value={pingHost}
                 disabled={pingRunning || pingBusy || !pingReady} onChange={(e) => setPingHost(e.target.value)} placeholder="主机名或 IP" />
          <button className={`btn sm ${pingRunning ? "danger" : "primary"}`} onClick={pingToggle} disabled={pingBusy || !pingReady}>
            <Icon name={pingRunning ? "stop" : "play"} size={13} />
            {pingRunning ? "停止" : "开始监测"}
          </button>
        </div>
        <PingDots samples={pingSamples} height={170} />
        {pStats && (
          <div className="row gap8 wrap mt8" style={{ fontSize: 12 }}>
            <span className="chip cyan">最新 {pStats.last != null ? `${pStats.last.toFixed(1)} ms` : "超时"}</span>
            <span className="chip">平均 {pStats.avg != null ? `${pStats.avg.toFixed(1)} ms` : "—"}</span>
            <span className="chip">最小 {pStats.min != null ? `${pStats.min.toFixed(1)}` : "—"} / 最大 {pStats.max != null ? `${pStats.max.toFixed(1)}` : "—"} ms</span>
            <span className="chip">抖动 {pStats.jitter.toFixed(1)} ms</span>
            <span className={`chip ${pStats.loss > 0 ? "red" : "green"}`}>丢包 {pStats.loss.toFixed(0)}%({pStats.n} 样本)</span>
          </div>
        )}
        {!pingRunning && pingSamples.length === 0 && (
          <div className="faint mt8" style={{ fontSize: 11.5 }}>输入目标后点击开始监测:每个点 = 一次真实 ICMP 探测,红叉 = 丢包,纵轴自适应 P95。</div>
        )}
      </div>

      {/* 网卡明细 */}
      <div className="card mt16" style={{ padding: 0 }}>
        <div className="chart-title" style={{ padding: "14px 18px 0" }}><Icon name="layers" size={15} /> 网卡明细</div>
        <div className="table-wrap mt8" style={{ maxHeight: 280, overflowY: "auto" }}>
          <table className="table">
            <thead>
              <tr><th>网卡</th><th>接收速率</th><th>发送速率</th><th>累计接收</th><th>累计发送</th><th>活动</th></tr>
            </thead>
            <tbody>
              {topIfaces.map((i) => (
                <tr key={i.name}>
                  <td className="mono" style={{ fontSize: 12 }}>{i.name}</td>
                  <td className="num">{fmtBytesPerSec(i.rx_bps)}</td>
                  <td className="num">{fmtBytesPerSec(i.tx_bps)}</td>
                  <td className="num">{fmtBytes(i.rx_total)}</td>
                  <td className="num">{fmtBytes(i.tx_total)}</td>
                  <td>{i.up ? <span className="chip green">活跃</span> : <span className="chip">空闲</span>}</td>
                </tr>
              ))}
            </tbody>
          </table>
          {topIfaces.length === 0 && (
            <div className="empty"><Icon name="activity" size={30} /><span>{settings?.monitor_enabled ? "正在采集…" : "后台监控已关闭"}</span></div>
          )}
        </div>
      </div>
    </div>
  );
}

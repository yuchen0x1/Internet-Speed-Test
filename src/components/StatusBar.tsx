// 底部状态栏:连接状态 / 当前节点 / 引擎状态 / 最近测速
import React from "react";
import { onMonitorTick } from "../api";
import { useApp } from "../store/app";
import { useTest } from "../store/test";
import { fmtBytesPerSec } from "../lib/format";
import { Icon } from "./Icon";
import type { MonitorTick } from "../types";

const PHASE_TEXT: Record<string, string> = {
  idle: "空闲",
  preparing: "准备中",
  latency: "延迟测试",
  download: "下载测速",
  upload: "上传测速",
  quality: "质量分析",
  finished: "已完成",
};

export function StatusBar() {
  const net = useApp((s) => s.net);
  const settings = useApp((s) => s.settings);
  // 逐字段订阅:仅本栏用到的测速字段变化时重渲染
  const phase = useTest((s) => s.phase);
  const running = useTest((s) => s.running);
  const result = useTest((s) => s.result);
  const node = useTest((s) => s.node);
  const [tick, setTick] = React.useState<MonitorTick | null>(null);

  React.useEffect(() => {
    let alive = true;
    let un: (() => void) | null = null;
    onMonitorTick((t) => alive && setTick(t)).then((off) => {
      if (!alive) off();
      else un = off;
    });
    return () => {
      alive = false;
      un?.();
    };
  }, []);

  const lastTime = result?.finished_at;
  const engineText = running
    ? `测速中 · ${PHASE_TEXT[phase] ?? phase}`
    : result
      ? result.status === "completed"
        ? `已完成 · ${result.download_mbps.toFixed(1)} Mbps ↓`
        : result.status === "cancelled"
          ? "已取消"
          : "失败"
      : "空闲";

  return (
    <footer className="statusbar">
      <div className="seg">
        <span className={`status-dot ${net.connected ? "ok" : "err"} ${running ? "pulse" : ""}`} />
        {net.connected ? "已连接" : "未连接"}
      </div>
      <div className="seg">
        <Icon name="server" size={13} />
        {node ? <b>{node.name}</b> : "自动选择节点"}
      </div>
      <div className="seg">
        <Icon name="zap" size={13} />
        测速引擎:<b>{engineText}</b>
      </div>
      <div className="grow" />
      {settings?.show_traffic && tick && (
        <div className="seg mono">
          <span style={{ color: "var(--cyan)" }}>↓ {fmtBytesPerSec(tick.rx_bps)}</span>
          <span style={{ color: "var(--purple)" }}>↑ {fmtBytesPerSec(tick.tx_bps)}</span>
        </div>
      )}
      <div className="seg">
        <Icon name="clock" size={13} />
        最近测速:<b>{lastTime ?? "暂无"}</b>
      </div>
    </footer>
  );
}

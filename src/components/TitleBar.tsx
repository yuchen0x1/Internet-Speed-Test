// 自定义标题栏:LOGO / 名称 / 网络状态 / 实时活动柱 / 窗口控制(拖拽区)
import { useEffect, useRef, useState } from "react";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { useApp } from "../store/app";
import { fmtSpeedMbps } from "../lib/format";
import { onMonitorTick } from "../api";
import { ActivityBars } from "./ActivityBars";
import { Icon } from "./Icon";
import type { MonitorTick } from "../types";
import { useI18n } from "../i18n";

export function TitleBar() {
  const { t, locale } = useI18n();
  const net = useApp((s) => s.net);
  const settings = useApp((s) => s.settings);
  const cur = getCurrentWindow();
  const [tick, setTick] = useState<MonitorTick | null>(null);
  // 最近 6 秒真实速率历史(驱动活动柱,与 mic 活动柱同一数据管线)
  const rxHist = useRef<number[]>([]);
  const txHist = useRef<number[]>([]);

  useEffect(() => {
    let alive = true;
    let un: (() => void) | null = null;
    onMonitorTick((t) => {
      if (!alive) return;
      rxHist.current = [...rxHist.current, t.rx_bps].slice(-6);
      txHist.current = [...txHist.current, t.tx_bps].slice(-6);
      setTick(t);
    }).then((off) => {
      if (!alive) off();
      else un = off;
    });
    return () => {
      alive = false;
      un?.();
    };
  }, []);

  const kindIcon = net.kind === "wifi" ? "wifi" : net.kind === "ethernet" ? "ethernet" : "globe";
  const kindText =
    net.kind === "wifi" ? "Wi-Fi" : net.kind === "ethernet" ? (locale === "zh-CN" ? "以太网" : "Ethernet") : net.kind === "cellular" ? (locale === "zh-CN" ? "移动网络" : "Mobile") : t("offline");
  const link = net.link_mbps ? `${net.link_mbps >= 1000 ? (net.link_mbps / 1000).toFixed(1) + " Gbps" : net.link_mbps + " Mbps"}` : null;
  const traffic = settings?.show_traffic && net.connected ? tick : null;
  const rxTraffic = traffic ? fmtSpeedMbps((traffic.rx_bps * 8) / 1e6) : null;
  const txTraffic = traffic ? fmtSpeedMbps((traffic.tx_bps * 8) / 1e6) : null;

  return (
    <header className="titlebar" data-tauri-drag-region role="banner" aria-label={t("windowTitle")}>
      <div className="brand" data-tauri-drag-region>
        <div className="brand-logo">
          <Icon name="gauge" size={17} />
        </div>
        <span className="brand-name">NetVision Pro</span>
      </div>

      <div className={`net-chip ${net.connected ? "up" : "down"}`} role="status" aria-live="polite">
        <span className="dot" />
        <Icon name={kindIcon} size={14} />
        <span>
          {net.connected ? (
            <>
              <b>{kindText}</b>
              {net.iface ? ` · ${net.iface}` : ""}
              {link ? ` · ${link}` : ""}
            </>
          ) : (
            t("offline")
          )}
        </span>
      </div>

      {traffic && (
        <div className="net-chip">
          <ActivityBars history={rxHist.current} color="var(--cyan)" height={16} bars={6} />
          <span className="mono">{rxTraffic?.value} {rxTraffic?.unit} ↓</span>
          <ActivityBars history={txHist.current} color="var(--purple)" height={16} bars={6} />
          <span className="mono">{txTraffic?.value} {txTraffic?.unit} ↑</span>
        </div>
      )}

      <div className="titlebar-spacer" data-tauri-drag-region />

      <div className="win-controls">
        <button className="win-btn" onClick={() => cur.minimize()} aria-label="最小化">
          <Icon name="minus" size={15} />
        </button>
        <button className="win-btn" onClick={() => cur.toggleMaximize()} aria-label="最大化/还原">
          <Icon name="square" size={13} />
        </button>
        <button className="win-btn close" onClick={() => cur.close()} aria-label="关闭">
          <Icon name="close" size={15} />
        </button>
      </div>
    </header>
  );
}

// 连续 Ping 实时点图(gping 风格):每个样本一个点,y=延迟,颜色按延迟分级,
// 丢失画为底部红叉;统计实时计算。数据源:pingmon://tick 真实 ICMP。
import { useEffect, useRef, useState } from "react";

export interface PingSample {
  ts: number;
  rtt: number | null;
}

const WINDOW = 90; // 可见样本数

export function PingDots(props: { samples: PingSample[]; height?: number }) {
  const { samples, height = 170 } = props;
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const wrapRef = useRef<HTMLDivElement>(null);
  const [size, setSize] = useState({ w: 600, h: height });

  useEffect(() => {
    const el = wrapRef.current;
    if (!el) return;
    const ro = new ResizeObserver((es) => {
      setSize({ w: Math.max(120, es[0].contentRect.width), h: height });
    });
    ro.observe(el);
    return () => ro.disconnect();
  }, [height]);

  useEffect(() => {
    const canvas = canvasRef.current;
    if (!canvas) return;
    const dpr = window.devicePixelRatio || 1;
    canvas.width = size.w * dpr;
    canvas.height = size.h * dpr;
    const ctx = canvas.getContext("2d");
    if (!ctx) return;
    ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
    ctx.clearRect(0, 0, size.w, size.h);

    const padL = 40, padR = 8, padT = 10, padB = 16;
    const cw = size.w - padL - padR, ch = size.h - padT - padB;
    const visible = samples.slice(-WINDOW);

    // y 轴:自适应 p95(避免单尖峰压扁)
    const rtts = visible.filter((s) => s.rtt != null).map((s) => s.rtt!) ;
    const sorted = [...rtts].sort((a, b) => a - b);
    const p95 = sorted.length ? sorted[Math.floor(sorted.length * 0.95)] : 50;
    const yMax = Math.max(20, Math.ceil((p95 * 1.25) / 10) * 10);

    // 网格 + y 轴
    ctx.font = "10px 'Cascadia Mono', Consolas, monospace";
    for (let i = 0; i <= 4; i++) {
      const v = (yMax / 4) * i;
      const y = padT + ch - (v / yMax) * ch;
      ctx.strokeStyle = "rgba(122,176,255,.08)";
      ctx.beginPath();
      ctx.moveTo(padL, y); ctx.lineTo(size.w - padR, y);
      ctx.stroke();
      ctx.fillStyle = "rgba(139,163,199,.65)";
      ctx.textAlign = "right";
      ctx.fillText(`${v.toFixed(0)}`, padL - 5, y + 3);
    }

    // 平均线
    if (rtts.length) {
      const avg = rtts.reduce((a, b) => a + b, 0) / rtts.length;
      const y = padT + ch - (avg / yMax) * ch;
      ctx.save();
      ctx.strokeStyle = "rgba(34,211,238,.45)";
      ctx.setLineDash([5, 4]);
      ctx.beginPath(); ctx.moveTo(padL, y); ctx.lineTo(size.w - padR, y); ctx.stroke();
      ctx.restore();
      ctx.fillStyle = "rgba(34,211,238,.8)";
      ctx.textAlign = "left";
      ctx.fillText(`avg ${avg.toFixed(1)}`, padL + 4, y - 4);
    }

    // 点
    visible.forEach((s, i) => {
      const x = padL + (i / Math.max(1, WINDOW - 1)) * cw;
      if (s.rtt == null) {
        // 丢失:底部红叉
        const y = padT + ch - 4;
        ctx.strokeStyle = "rgba(248,113,113,.95)";
        ctx.lineWidth = 1.6;
        ctx.beginPath();
        ctx.moveTo(x - 3, y - 3); ctx.lineTo(x + 3, y + 3);
        ctx.moveTo(x + 3, y - 3); ctx.lineTo(x - 3, y + 3);
        ctx.stroke();
        ctx.lineWidth = 1;
        return;
      }
      const y = padT + ch - (Math.min(s.rtt, yMax) / yMax) * ch;
      // 颜色分级:绿(<60) → 青(<120) → 琥珀(<200) → 红
      const c = s.rtt < 60 ? "#34d399" : s.rtt < 120 ? "#22d3ee" : s.rtt < 200 ? "#fbbf24" : "#f87171";
      ctx.beginPath();
      ctx.arc(x, y, 2.6, 0, Math.PI * 2);
      ctx.fillStyle = c;
      ctx.shadowColor = c;
      ctx.shadowBlur = 5;
      ctx.fill();
      ctx.shadowBlur = 0;
    });
  }, [samples, size]);

  return (
    <div ref={wrapRef} style={{ position: "relative", width: "100%", height }}>
      <canvas ref={canvasRef} style={{ width: "100%", height, display: "block" }} />
    </div>
  );
}

/** 实时统计(滚动窗口) */
export function pingStats(samples: PingSample[], window = 60) {
  const recent = samples.slice(-window);
  const ok = recent.filter((s) => s.rtt != null).map((s) => s.rtt!);
  const lost = recent.length - ok.length;
  if (!recent.length) return null;
  const avg = ok.length ? ok.reduce((a, b) => a + b, 0) / ok.length : null;
  const jitter = ok.length > 1
    ? ok.slice(1).reduce((acc, v, i) => acc + Math.abs(v - ok[i]), 0) / (ok.length - 1)
    : 0;
  return {
    n: recent.length,
    loss: (lost / recent.length) * 100,
    avg,
    min: ok.length ? Math.min(...ok) : null,
    max: ok.length ? Math.max(...ok) : null,
    jitter,
    last: ok.length ? ok[ok.length - 1] : null,
  };
}

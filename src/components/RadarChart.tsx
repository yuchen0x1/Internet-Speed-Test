// 网络五维雷达图(Canvas):延迟/抖动/丢包/下载/上传,分数 0-1 归一后绘制。
// 低分越差的维度(延迟/抖动/丢包)按 1 - value/max 反转,保证"越大越好"。
import { useEffect, useRef } from "react";

export interface RadarMetric {
  label: string;
  /** 原始值 */
  value: number;
  /** 满分参考值(超出按 1 截断) */
  max: number;
  /** true 表示该维度越低越好(自动反转) */
  lowerBetter?: boolean;
  /** 显示文本(如 "181.7 ms") */
  display: string;
}

export function RadarChart(props: { metrics: RadarMetric[]; size?: number; color?: string }) {
  const { metrics, size = 260, color = "#22d3ee" } = props;
  const ref = useRef<HTMLCanvasElement>(null);

  const scores = metrics.map((m) => {
    const raw = Math.min(1, Math.max(0, m.value / m.max));
    return m.lowerBetter ? 1 - raw : raw;
  });

  useEffect(() => {
    const canvas = ref.current;
    if (!canvas) return;
    const dpr = window.devicePixelRatio || 1;
    canvas.width = size * dpr;
    canvas.height = size * dpr;
    const ctx = canvas.getContext("2d");
    if (!ctx) return;
    ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
    ctx.clearRect(0, 0, size, size);

    const cx = size / 2, cy = size / 2 + 4;
    const R = size / 2 - 34;
    const n = metrics.length;
    if (n < 3) return;
    const ang = (i: number) => (Math.PI * 2 * i) / n - Math.PI / 2;

    // 网格环(4 层)
    for (let ring = 1; ring <= 4; ring++) {
      ctx.beginPath();
      for (let i = 0; i <= n; i++) {
        const a = ang(i % n);
        const r = (R * ring) / 4;
        const x = cx + Math.cos(a) * r, y = cy + Math.sin(a) * r;
        i === 0 ? ctx.moveTo(x, y) : ctx.lineTo(x, y);
      }
      ctx.strokeStyle = "rgba(122,176,255,.14)";
      ctx.stroke();
    }
    // 轴线 + 标签
    for (let i = 0; i < n; i++) {
      const a = ang(i);
      ctx.beginPath();
      ctx.moveTo(cx, cy);
      ctx.lineTo(cx + Math.cos(a) * R, cy + Math.sin(a) * R);
      ctx.strokeStyle = "rgba(122,176,255,.14)";
      ctx.stroke();
      const lx = cx + Math.cos(a) * (R + 18);
      const ly = cy + Math.sin(a) * (R + 18);
      ctx.fillStyle = "rgba(139,163,199,.95)";
      ctx.font = "11px 'Segoe UI', sans-serif";
      ctx.textAlign = "center";
      ctx.fillText(metrics[i].label, lx, ly);
      ctx.font = "10px 'Cascadia Mono', monospace";
      ctx.fillStyle = "rgba(91,107,133,.9)";
      ctx.fillText(metrics[i].display, lx, ly + 13);
    }

    // 数据多边形(渐变填充 + 发光描边)
    const grad = ctx.createLinearGradient(cx - R, cy - R, cx + R, cy + R);
    grad.addColorStop(0, "rgba(34,211,238,.34)");
    grad.addColorStop(1, "rgba(167,139,250,.30)");
    ctx.beginPath();
    for (let i = 0; i <= n; i++) {
      const a = ang(i % n);
      const r = R * Math.max(0.04, scores[i % n]);
      const x = cx + Math.cos(a) * r, y = cy + Math.sin(a) * r;
      i === 0 ? ctx.moveTo(x, y) : ctx.lineTo(x, y);
    }
    ctx.closePath();
    ctx.fillStyle = grad;
    ctx.fill();
    ctx.strokeStyle = color;
    ctx.lineWidth = 2;
    ctx.shadowColor = color;
    ctx.shadowBlur = 10;
    ctx.stroke();
    ctx.shadowBlur = 0;
    // 顶点
    for (let i = 0; i < n; i++) {
      const a = ang(i);
      const r = R * Math.max(0.04, scores[i]);
      ctx.beginPath();
      ctx.arc(cx + Math.cos(a) * r, cy + Math.sin(a) * r, 3, 0, Math.PI * 2);
      ctx.fillStyle = "#fff";
      ctx.fill();
    }
  }, [metrics, scores.join(","), size, color]);

  return <canvas ref={ref} style={{ width: size, height: size }} />;
}

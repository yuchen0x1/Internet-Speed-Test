// 专业实时曲线图:Canvas 渲染,渐变填充,均值线,峰值标记,悬停十字线 + 数值提示
import React, { useEffect, useRef, useState } from "react";

export interface Series {
  name: string;
  color: string;
  data: [number, number][]; // [x, y] x 单位:秒
  fill?: boolean;
  dashed?: boolean;
}

function niceTicks(max: number): number[] {
  if (max <= 0) return [0, 1];
  const raw = max / 4;
  const pow = Math.pow(10, Math.floor(Math.log10(raw)));
  const n = raw / pow;
  const step = (n <= 1 ? 1 : n <= 2 ? 2 : n <= 2.5 ? 2.5 : n <= 5 ? 5 : 10) * pow;
  const ticks: number[] = [];
  for (let v = 0; v <= max * 1.0001; v += step) ticks.push(Number(v.toFixed(6)));
  return ticks;
}

export function LineChart(props: {
  series: Series[];
  height?: number;
  yUnit?: string;
  /** x 轴窗口(秒);0 表示自适应数据范围 */
  windowSecs?: number;
  avgLine?: boolean;
  peakMark?: boolean;
  anomalyThreshold?: number;
  emptyText?: string;
}) {
  const {
    series, height = 190, yUnit = "", windowSecs = 0, avgLine = false, peakMark = false,
    anomalyThreshold, emptyText = "暂无数据",
  } = props;
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const wrapRef = useRef<HTMLDivElement>(null);
  const [hover, setHover] = useState<{ x: number; y: number; items: { name: string; color: string; v: number }[]; label: string } | null>(null);
  const [size, setSize] = useState({ w: 600, h: height });

  useEffect(() => {
    const el = wrapRef.current;
    if (!el) return;
    const ro = new ResizeObserver((entries) => {
      const { width } = entries[0].contentRect;
      setSize({ w: Math.max(120, width), h: height });
    });
    ro.observe(el);
    return () => ro.disconnect();
  }, [height]);

  // 绘制
  useEffect(() => {
    const canvas = canvasRef.current;
    if (!canvas) return;
    const dpr = window.devicePixelRatio || 1;
    const { w, h } = size;
    canvas.width = w * dpr;
    canvas.height = h * dpr;
    const ctx = canvas.getContext("2d");
    if (!ctx) return;
    ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
    ctx.clearRect(0, 0, w, h);

    const padL = 46, padR = 14, padT = 14, padB = 22;
    const cw = w - padL - padR, ch = h - padT - padB;

    // 数据范围
    let xMin = Infinity, xMax = -Infinity, yMax = 0;
    let count = 0;
    for (const s of series) {
      for (const [x, y] of s.data) {
        if (x < xMin) xMin = x;
        if (x > xMax) xMax = x;
        if (y > yMax) yMax = y;
        count++;
      }
    }
    if (windowSecs > 0) {
      xMax = Math.max(xMax, windowSecs);
      xMin = Math.max(0, xMax - windowSecs);
    }
    if (!isFinite(xMin)) { xMin = 0; xMax = Math.max(1, windowSecs); }
    if (xMax - xMin <= 0) xMax = xMin + 1;
    const yTop = Math.max(niceTicks(yMax).slice(-1)[0] ?? 1, 0.5);

    // 网格 + Y 轴刻度
    const ticks = niceTicks(yTop);
    ctx.font = "10.5px 'Cascadia Mono', Consolas, monospace";
    ctx.fillStyle = "rgba(139,163,199,.65)";
    ctx.strokeStyle = "rgba(122,176,255,.08)";
    ctx.lineWidth = 1;
    ctx.textAlign = "right";
    for (const t of ticks) {
      const y = padT + ch - (t / yTop) * ch;
      ctx.beginPath();
      ctx.moveTo(padL, y); ctx.lineTo(w - padR, y);
      ctx.stroke();
      const label = t >= 1000 ? `${(t / 1000).toFixed(1)}k` : String(Number(t.toFixed(2)));
      ctx.fillText(label, padL - 7, y + 3.5);
    }
    // X 轴
    ctx.textAlign = "center";
    const xStep = (xMax - xMin) / 4;
    for (let i = 0; i <= 4; i++) {
      const xv = xMin + xStep * i;
      ctx.fillText(`${xv.toFixed(xStep < 1 ? 1 : 0)}s`, padL + (xv - xMin) / (xMax - xMin) * cw, h - 6);
    }
    // 单位
    ctx.textAlign = "left";
    ctx.fillStyle = "rgba(91,107,133,.8)";
    ctx.fillText(yUnit, 4, 10);

    if (count === 0) {
      ctx.fillStyle = "rgba(91,107,133,.7)";
      ctx.textAlign = "center";
      ctx.font = "12px 'Segoe UI', sans-serif";
      ctx.fillText(emptyText, w / 2, h / 2);
      return;
    }

    const toPx = (x: number, y: number): [number, number] => [
      padL + ((x - xMin) / (xMax - xMin)) * cw,
      padT + ch - (Math.min(y, yTop) / yTop) * ch,
    ];

    // 平滑曲线绘制
    const drawSeries = (s: Series) => {
      if (s.data.length === 0) return;
      ctx.save();
      // 渐变填充
      if (s.fill) {
        const grad = ctx.createLinearGradient(0, padT, 0, padT + ch);
        grad.addColorStop(0, s.color + "4d");
        grad.addColorStop(1, s.color + "00");
        ctx.beginPath();
        const pts = s.data.map(([x, y]) => toPx(x, y));
        ctx.moveTo(pts[0][0], padT + ch);
        for (const [px, py] of pts) ctx.lineTo(px, py);
        ctx.lineTo(pts[pts.length - 1][0], padT + ch);
        ctx.closePath();
        ctx.fillStyle = grad;
        ctx.fill();
      }
      // 线
      ctx.beginPath();
      const pts = s.data.map(([x, y]) => toPx(x, y));
      ctx.moveTo(pts[0][0], pts[0][1]);
      for (let i = 1; i < pts.length; i++) {
        const [px, py] = pts[i];
        const [ppx, ppy] = pts[i - 1];
        const mx = (ppx + px) / 2;
        ctx.bezierCurveTo(mx, ppy, mx, py, px, py);
      }
      ctx.strokeStyle = s.color;
      ctx.lineWidth = 1.8;
      ctx.setLineDash(s.dashed ? [4, 4] : []);
      ctx.shadowColor = s.color + "66";
      ctx.shadowBlur = 6;
      ctx.stroke();
      ctx.restore();
    };
    for (const s of series) drawSeries(s);

    // 均值参考线
    if (avgLine) {
      for (const s of series) {
        if (s.data.length < 2) continue;
        const avg = s.data.reduce((a, p) => a + p[1], 0) / s.data.length;
        const y = padT + ch - (avg / yTop) * ch;
        ctx.save();
        ctx.strokeStyle = s.color + "88";
        ctx.setLineDash([6, 5]);
        ctx.lineWidth = 1;
        ctx.beginPath();
        ctx.moveTo(padL, y); ctx.lineTo(w - padR, y);
        ctx.stroke();
        ctx.fillStyle = s.color + "cc";
        ctx.font = "10px 'Cascadia Mono', monospace";
        ctx.textAlign = "left";
        ctx.fillText(`avg ${avg.toFixed(1)}`, padL + 4, y - 4);
        ctx.restore();
      }
    }

    // 峰值标记
    if (peakMark) {
      for (const s of series) {
        if (s.data.length < 2) continue;
        let peak = s.data[0];
        for (const p of s.data) if (p[1] > peak[1]) peak = p;
        const [px, py] = toPx(peak[0], peak[1]);
        ctx.save();
        ctx.beginPath();
        ctx.arc(px, py, 3.2, 0, Math.PI * 2);
        ctx.fillStyle = "#fff";
        ctx.shadowColor = s.color;
        ctx.shadowBlur = 8;
        ctx.fill();
        ctx.fillStyle = s.color;
        ctx.font = "10px 'Cascadia Mono', monospace";
        ctx.textAlign = "right";
        ctx.fillText(`peak ${peak[1].toFixed(1)}`, px - 8, py - 6);
        ctx.restore();
      }
    }

    // 异常点标记(延迟尖峰)
    if (anomalyThreshold && anomalyThreshold > 0) {
      for (const s of series) {
        for (const [x, y] of s.data) {
          if (y > anomalyThreshold) {
            const [px, py] = toPx(x, y);
            ctx.beginPath();
            ctx.arc(px, py, 2.6, 0, Math.PI * 2);
            ctx.fillStyle = "rgba(248,113,113,.9)";
            ctx.shadowColor = "rgba(248,113,113,.8)";
            ctx.shadowBlur = 6;
            ctx.fill();
            ctx.shadowBlur = 0;
          }
        }
      }
    }

    // 悬停十字线(重绘于最上层)
    if (hover) {
      const hx = hover.x;
      ctx.save();
      ctx.strokeStyle = "rgba(232,241,255,.25)";
      ctx.setLineDash([3, 3]);
      ctx.beginPath();
      ctx.moveTo(hx, padT); ctx.lineTo(hx, padT + ch);
      ctx.stroke();
      ctx.restore();
    }
  }, [series, size, hover, windowSecs, avgLine, peakMark, anomalyThreshold, yUnit, emptyText]);

  // 悬停取值
  const onMove = (e: React.MouseEvent) => {
    const canvas = canvasRef.current;
    if (!canvas) return;
    const rect = canvas.getBoundingClientRect();
    const mx = e.clientX - rect.left;
    const padL = 46, padR = 14;
    const cw = size.w - padL - padR;
    if (mx < padL || mx > size.w - padR) { setHover(null); return; }
    // x 范围
    let xMin = Infinity, xMax = -Infinity;
    let yMax = 0;
    for (const s of series) for (const [x, y] of s.data) {
      if (x < xMin) xMin = x;
      if (x > xMax) xMax = x;
      if (y > yMax) yMax = y;
    }
    if (!isFinite(xMin)) return;
    if (windowSecs > 0) { xMax = Math.max(xMax, windowSecs); xMin = Math.max(0, xMax - windowSecs); }
    if (xMax <= xMin) xMax = xMin + 1;
    const xv = xMin + ((mx - padL) / cw) * (xMax - xMin);
    const items: { name: string; color: string; v: number }[] = [];
    for (const s of series) {
      if (s.data.length === 0) continue;
      let best = s.data[0];
      let bestD = Infinity;
      for (const p of s.data) {
        const d = Math.abs(p[0] - xv);
        if (d < bestD) { bestD = d; best = p; }
      }
      items.push({ name: s.name, color: s.color, v: best[1] });
    }
    setHover({ x: mx, y: e.clientY - rect.top, items, label: `${xv.toFixed(1)}s` });
  };

  return (
    <div ref={wrapRef} className="linechart" style={{ position: "relative", width: "100%", height }}>
      <canvas
        ref={canvasRef}
        style={{ width: "100%", height, display: "block", cursor: "crosshair" }}
        onMouseMove={onMove}
        onMouseLeave={() => setHover(null)}
      />
      {/* 图例 */}
      <div className="chart-legend">
        {series.filter((s) => s.data.length > 0 || series.length === 1).map((s) => (
          <span key={s.name} className="legend-item">
            <i style={{ background: s.color, boxShadow: `0 0 6px ${s.color}` }} />
            {s.name}
          </span>
        ))}
      </div>
      {hover && hover.items.length > 0 && (
        <div
          className="chart-tooltip"
          style={{
            left: Math.min(hover.x + 12, size.w - 130),
            top: Math.max(8, hover.y - 46),
          }}
        >
          <div className="tt-label">{hover.label}</div>
          {hover.items.map((it) => (
            <div key={it.name} className="tt-row">
              <i style={{ background: it.color }} />
              <span>{it.name}</span>
              <b>{it.v >= 1000 ? (it.v / 1000).toFixed(2) + "k" : it.v.toFixed(2)}</b>
              <span className="tt-unit">{yUnit}</span>
            </div>
          ))}
        </div>
      )}
    </div>
  );
}

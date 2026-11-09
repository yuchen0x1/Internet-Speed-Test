// 节点延迟雷达(科幻扫描风):同心环 = 延迟档位(50/100/150/200ms),
// 扫描线旋转,节点光点按延迟定位、按可用性着色。角度由节点 ID 稳定散列,
// 半径才是真实数据(延迟);角度仅为布局,不表达地理方位。
import { useEffect, useRef } from "react";
import { useMotionEnabled } from "../lib/motion";
import type { ServerNodeInfo } from "../types";

export function NodeRadar(props: { nodes: ServerNodeInfo[]; size?: number }) {
  const { nodes, size = 300 } = props;
  const ref = useRef<HTMLCanvasElement>(null);
  const angleRef = useRef(0);
  const motionOn = useMotionEnabled();

  // 稳定伪角度:节点 ID 哈希 → 0..2π
  const angleOf = (id: string) => {
    let h = 0;
    for (let i = 0; i < id.length; i++) h = (h * 31 + id.charCodeAt(i)) >>> 0;
    return ((h % 3600) / 3600) * Math.PI * 2;
  };

  useEffect(() => {
    const canvas = ref.current;
    if (!canvas) return;
    const dpr = window.devicePixelRatio || 1;
    canvas.width = size * dpr;
    canvas.height = size * dpr;
    const ctx = canvas.getContext("2d");
    if (!ctx) return;
    ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
    let raf = 0;

    const draw = () => {
      const cx = size / 2, cy = size / 2;
      const R = size / 2 - 26;
      ctx.clearRect(0, 0, size, size);

      // 背景微光
      const bg = ctx.createRadialGradient(cx, cy, 10, cx, cy, R);
      bg.addColorStop(0, "rgba(34,211,238,.06)");
      bg.addColorStop(1, "rgba(34,211,238,0)");
      ctx.fillStyle = bg;
      ctx.fillRect(0, 0, size, size);

      // 同心延迟环 50/100/150/200ms
      ctx.font = "9.5px 'Cascadia Mono', monospace";
      for (let i = 1; i <= 4; i++) {
        const r = (R * i) / 4;
        ctx.beginPath();
        ctx.arc(cx, cy, r, 0, Math.PI * 2);
        ctx.strokeStyle = "rgba(122,176,255,.16)";
        ctx.stroke();
        ctx.fillStyle = "rgba(91,107,133,.8)";
        ctx.textAlign = "left";
        ctx.fillText(`${i * 50}`, cx + 3, cy - r + 11);
      }
      // 十字准线
      ctx.strokeStyle = "rgba(122,176,255,.10)";
      ctx.beginPath();
      ctx.moveTo(cx - R, cy); ctx.lineTo(cx + R, cy);
      ctx.moveTo(cx, cy - R); ctx.lineTo(cx, cy + R);
      ctx.stroke();

      // 扫描扇形(旋转)
      angleRef.current = (angleRef.current + 0.022) % (Math.PI * 2);
      const a0 = angleRef.current;
      const g = ctx;
      g.save();
      g.beginPath();
      g.moveTo(cx, cy);
      g.arc(cx, cy, R, a0 - 0.9, a0);
      g.closePath();
      const wgrad = g.createRadialGradient(cx, cy, 8, cx, cy, R);
      wgrad.addColorStop(0, "rgba(34,211,238,.02)");
      wgrad.addColorStop(1, "rgba(34,211,238,.16)");
      g.fillStyle = wgrad;
      g.fill();
      // 扫描前沿亮线
      g.beginPath();
      g.moveTo(cx, cy);
      g.lineTo(cx + Math.cos(a0) * R, cy + Math.sin(a0) * R);
      g.strokeStyle = "rgba(34,211,238,.55)";
      g.lineWidth = 1.4;
      g.stroke();
      g.lineWidth = 1;
      g.restore();

      // 节点光点:半径 = 延迟(50ms/环)
      for (const n of nodes) {
        if (n.alive !== true || n.latency_ms == null) continue;
        const a = angleOf(n.id);
        const r = R * Math.min(1, n.latency_ms / 200);
        const x = cx + Math.cos(a) * r, y = cy + Math.sin(a) * r;
        const isNear = ((a0 - a + Math.PI * 4) % (Math.PI * 2)) < 0.9;
        ctx.beginPath();
        ctx.arc(x, y, isNear ? 4 : 3, 0, Math.PI * 2);
        ctx.fillStyle = n.latency_ms < 60 ? "#34d399" : n.latency_ms < 120 ? "#22d3ee" : "#38bdf8";
        ctx.shadowColor = ctx.fillStyle as string;
        ctx.shadowBlur = isNear ? 10 : 6;
        ctx.fill();
        ctx.shadowBlur = 0;
      }

      // 中心点
      ctx.beginPath();
      ctx.arc(cx, cy, 3.4, 0, Math.PI * 2);
      ctx.fillStyle = "#e8f1ff";
      ctx.shadowColor = "#22d3ee";
      ctx.shadowBlur = 8;
      ctx.fill();
      ctx.shadowBlur = 0;

      if (motionOn) raf = requestAnimationFrame(draw);
    };
    // 动效关闭(设置开关或系统减少动态效果):只画一帧静态雷达,不进 rAF 循环
    if (motionOn) raf = requestAnimationFrame(draw);
    else draw();
    return () => cancelAnimationFrame(raf);
  }, [nodes, size, motionOn]);

  const probed = nodes.filter((n) => n.alive === true && n.latency_ms != null).length;
  return (
    <div style={{ position: "relative", width: size, margin: "0 auto" }}>
      <canvas ref={ref} style={{ width: size, height: size, display: "block" }} />
      <div className="faint" style={{ textAlign: "center", fontSize: 11, marginTop: -6 }}>
        同心环 = 延迟档位(ms) · 已探测 {probed}/{nodes.length} 节点 · 角度为布局非地理方位
      </div>
    </div>
  );
}

// 中央测速仪表盘:环形渐变弧 + 刻度 + 扫描光效 + 谐振子弹簧数字
import React from "react";
import { useSpringNumber, SPRING_BOUNCY, SPRING_HEAVY } from "../lib/motion";
import type { Phase } from "../types";

const PHASE_COLORS: Record<string, { a: string; b: string; glow: string }> = {
  idle: { a: "#22d3ee", b: "#a78bfa", glow: "rgba(34,211,238,.35)" },
  preparing: { a: "#7dd3fc", b: "#38bdf8", glow: "rgba(125,211,252,.4)" },
  latency: { a: "#38bdf8", b: "#7dd3fc", glow: "rgba(56,189,248,.45)" },
  download: { a: "#22d3ee", b: "#38bdf8", glow: "rgba(34,211,238,.5)" },
  upload: { a: "#a78bfa", b: "#8b5cf6", glow: "rgba(167,139,250,.5)" },
  quality: { a: "#34d399", b: "#22d3ee", glow: "rgba(52,211,153,.45)" },
  finished: { a: "#22d3ee", b: "#a78bfa", glow: "rgba(34,211,238,.4)" },
  failed: { a: "#f87171", b: "#fbbf24", glow: "rgba(248,113,113,.4)" },
};

/** 数值平滑:阻尼谐振子弹簧(微弹超调手感),替换简单 lerp */
function useAnimatedNumber(target: number): number {
  return useSpringNumber(target, SPRING_BOUNCY, 0.005);
}

export function Gauge(props: {
  phase: Phase;
  running: boolean;
  /** 当前展示值 */
  value: number;
  label: string;
  unit: string;
  sub?: string;
  /** 弧进度(0-1);未传则按 value/max */
  progress?: number;
  max: number;
  error?: string | null;
}) {
  const { phase, running, label, unit, sub, max, error } = props;
  const colors = PHASE_COLORS[phase] ?? PHASE_COLORS.idle;
  const animated = useAnimatedNumber(props.value);
  const rawPct =
    props.progress != null
      ? Math.min(1, Math.max(0, props.progress))
      : Math.min(1, props.value / max);
  // 弧长用零过冲弹簧:快且稳,无回弹
  const pct = useSpringNumber(rawPct, SPRING_HEAVY, 0.002);

  const R = 132;
  const CX = 160;
  const CY = 160;
  const SWEEP = 270; // 度
  const START = 135; // 从左下开始
  const C = 2 * Math.PI * R;
  const arcLen = (C * SWEEP) / 360;

  const ticks: React.ReactNode[] = [];
  for (let i = 0; i <= 30; i++) {
    const ang = ((START + (SWEEP * i) / 30) * Math.PI) / 180;
    const major = i % 5 === 0;
    const r1 = major ? 152 : 158;
    const r2 = 166;
    ticks.push(
      <line
        key={i}
        x1={CX + Math.cos(ang) * r1}
        y1={CY + Math.sin(ang) * r1}
        x2={CX + Math.cos(ang) * r2}
        y2={CY + Math.sin(ang) * r2}
        stroke={major ? "rgba(125,211,252,.5)" : "rgba(125,211,252,.18)"}
        strokeWidth={major ? 2.4 : 1.4}
        strokeLinecap="round"
      />,
    );
  }

  // 弹簧过冲可能短暂低于 0;速度/延迟不存在负值,展示层钳制(不影响均值计算)
  const displayValue = Math.max(0, animated);
  const valueStr =
    displayValue >= 1000 ? (displayValue / 1000).toFixed(2) : displayValue >= 100 ? displayValue.toFixed(1) : displayValue.toFixed(2);
  const isIdle = phase === "idle" || (phase === "finished" && !running);

  return (
    <div className={`gauge ${running ? "running" : ""} ${isIdle ? "idle" : ""}`}>
      <svg width="320" height="320" viewBox="0 0 320 320">
        <defs>
          <linearGradient id={`gaugeGrad-${phase}`} x1="0%" y1="100%" x2="100%" y2="0%">
            <stop offset="0%" stopColor={colors.a} />
            <stop offset="100%" stopColor={colors.b} />
          </linearGradient>
          <radialGradient id={`gaugeGlow-${phase}`} cx="50%" cy="50%" r="50%">
            <stop offset="0%" stopColor={colors.glow} />
            <stop offset="72%" stopColor="rgba(0,0,0,0)" />
          </radialGradient>
        </defs>

        {/* 呼吸光环 */}
        <circle className="gauge-breath" cx={CX} cy={CY} r="146" fill={`url(#gaugeGlow-${phase})`} />

        {/* 背景轨道 */}
        <circle
          cx={CX} cy={CY} r={R}
          fill="none"
          stroke="rgba(122,176,255,.10)"
          strokeWidth="14"
          strokeLinecap="round"
          strokeDasharray={`${arcLen} ${C}`}
          transform={`rotate(${START} ${CX} ${CY})`}
        />

        {/* 进度弧 */}
        <circle
          className="gauge-arc"
          cx={CX} cy={CY} r={R}
          fill="none"
          stroke={`url(#gaugeGrad-${phase})`}
          strokeWidth="14"
          strokeLinecap="round"
          strokeDasharray={`${arcLen} ${C}`}
          strokeDashoffset={arcLen * (1 - pct)}
          transform={`rotate(${START} ${CX} ${CY})`}
          style={{
            transition: "stroke-dashoffset 0.25s ease, stroke 0.5s ease",
            filter: `drop-shadow(0 0 10px ${colors.glow})`,
          }}
        />

        {ticks}

        {/* 端点光点 */}
        {pct > 0.005 && (
          <circle
            cx={CX + Math.cos(((START + SWEEP * pct) * Math.PI) / 180) * R}
            cy={CY + Math.sin(((START + SWEEP * pct) * Math.PI) / 180) * R}
            r="6.5"
            fill="#fff"
            style={{ filter: `drop-shadow(0 0 8px ${colors.a})`, transition: "all 0.25s ease" }}
          />
        )}

        <text x={CX} y={CY - 8} textAnchor="middle" className="gauge-num">
          {error ? "—" : valueStr}
        </text>
        <text x={CX} y={CY + 22} textAnchor="middle" className="gauge-unit">
          {error ? "错误" : unit}
        </text>
      </svg>

      {/* 扫描光效(仅运行中) */}
      {running && <div className="gauge-scan" />}
      <div className="gauge-phase">
        <span className="gauge-phase-dot" style={{ background: colors.a, boxShadow: `0 0 10px ${colors.a}` }} />
        {error ? "测速失败" : label}
      </div>
      {sub && <div className="gauge-sub">{sub}</div>}
    </div>
  );
}

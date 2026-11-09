// 测速氛围光效卡片 —— 复用 open-claude-ui(MIT)ReasoningAmbientGlow 的四层视觉物理:
// ① 1.8s ease-in-out 双层呼吸光晕  ② 三相位脉冲点(静息透明度 0.2857 黄金常数)
// ③ 非对称贝塞尔扫光文字  ④ 底部 1.1s 线性能量扫光条
import React from "react";

export function GlowCard(props: {
  active: boolean;
  message?: string;
  elapsedSec?: string;
  children: React.ReactNode;
  className?: string;
}) {
  const { active, message, elapsedSec, children, className = "" } = props;
  return (
    <div className={`card pad glow-card ${active ? "on" : ""} ${className}`}>
      {active && <div className="glow-field" aria-hidden />}
      {active && (
        <div className="glow-head">
          <span className="glow-dots" aria-hidden>
            <i /><i /><i />
          </span>
          <span className="glow-shimmer">{message}</span>
          {elapsedSec != null && <span className="glow-time mono">{elapsedSec}s</span>}
        </div>
      )}
      <div className="glow-body">{children}</div>
      {active && <div className="glow-sweep" aria-hidden />}
    </div>
  );
}

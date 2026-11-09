/**
 * 阻尼谐振子动效引擎 —— 改编自 Caho1/open-claude-ui(MIT License)的
 * CDS Motion Engine,去除 framer-motion 依赖,以 rAF 数值积分实现。
 *
 * 物理方程(二阶阻尼谐振子): m·x'' + c·x' + k·(x - target) = 0
 * 阻尼比 ζ = c / (2·√(m·k));ζ<1 欠阻尼(微弹超调)、ζ=1 临界、ζ>1 过阻尼(零过冲)
 *
 * 预设数值源自该项目对 Claude 桌面端 ion-dist 生产包的逆向观测值。
 */

import { useEffect, useRef, useState } from "react";
import { useApp } from "../store/app";

/** 系统级"减少动态效果"偏好 */
export function prefersReducedMotion(): boolean {
  return (
    typeof window !== "undefined" &&
    typeof window.matchMedia === "function" &&
    window.matchMedia("(prefers-reduced-motion: reduce)").matches
  );
}

/**
 * JS 动效总开关:设置里的"动画效果"+ 系统减少动态效果偏好。
 * 关闭时 rAF/interval 循环不启动,数值直接吸附目标值。
 */
export function useMotionEnabled(): boolean {
  const animations = useApp((s) => s.settings?.animations ?? true);
  return animations && !prefersReducedMotion();
}

export interface SpringParams {
  /** k: 弹性刚度 */
  k: number;
  /** c: 阻尼系数 */
  c: number;
  /** m: 质量 */
  m: number;
}

/** 高频微弹(ζ≈0.612,超调约 +8.8%)—— 用于大数字/仪表读数 */
export const SPRING_BOUNCY: SpringParams = { k: 600, c: 30, m: 1.0 };

/** 沉稳顺滑(ζ≈0.783,超调约 +1.9%)—— 用于常规过渡 */
export const SPRING_SNAPPY: SpringParams = { k: 320, c: 28, m: 1.0 };

/** 零过冲快速收敛(过阻尼 ζ≈3.65)—— 用于进度弧/宽度 */
export const SPRING_HEAVY: SpringParams = { k: 1200, c: 80, m: 0.1 };

/** CDS 贝塞尔缓动常量(用于 CSS transition) */
export const CDS_EASING = {
  OVERSHOOT: "cubic-bezier(0.34, 1.3, 0.64, 1)",
  EASE_OUT_EXPO: "cubic-bezier(0.16, 1, 0.3, 1)",
  DECEL: "cubic-bezier(0.165, 0.84, 0.44, 1)",
  SHARP_PRESS: "cubic-bezier(0.32, 0.72, 0, 1)",
} as const;

/**
 * 弹簧跟踪数值:目标值连续变化时(实时速度采样),以谐振子积分逼近,
 * 速度跨目标变化保持连续,产生有机的微超调手感。
 */
export function useSpringNumber(
  target: number,
  preset: SpringParams = SPRING_SNAPPY,
  precision = 0.01,
): number {
  const [display, setDisplay] = useState(target);
  const sim = useRef({ x: target, v: 0 });
  const targetRef = useRef(target);
  targetRef.current = target;
  const motionOn = useMotionEnabled();

  useEffect(() => {
    if (!motionOn) {
      // 动效关闭:不启动 rAF,直接吸附目标值(并复位弹簧,重新开启时从当前值出发)
      sim.current = { x: targetRef.current, v: 0 };
      setDisplay(targetRef.current);
      return;
    }
    let raf = 0;
    let last = performance.now();
    const tick = (now: number) => {
      const dt = Math.min(0.05, (now - last) / 1000);
      last = now;
      const s = sim.current;
      const t = targetRef.current;
      const a = (preset.k * (t - s.x) - preset.c * s.v) / preset.m;
      s.v += a * dt;
      s.x += s.v * dt;
      if (Math.abs(t - s.x) < precision && Math.abs(s.v) < precision * 10) {
        s.x = t;
        s.v = 0;
        setDisplay(t);
        return; // 收敛:停止 rAF,目标再变化时由 effect 重启
      }
      setDisplay(s.x);
      raf = requestAnimationFrame(tick);
    };
    raf = requestAnimationFrame(tick);
    return () => cancelAnimationFrame(raf);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [target, preset.k, preset.c, preset.m, precision, motionOn]);

  return motionOn ? display : target;
}

// 实时活动柱 —— 复用 open-claude-ui(MIT)LiveMicActivityBars 的数据管线:
// 归一化 → 噪声门限 → Gamma 幂律压缩(0.7 次幂强化弱信号)→ 平滑柱高。
// 数据源为真实流量历史(最近 N 秒网卡速率 / 测速实时采样),非装饰性动画。
import { CDS_EASING } from "../lib/motion";

const NOISE_FLOOR = 0.06;

export function ActivityBars(props: {
  /** 真实数据历史(最近 N 个采样),旧→新 */
  history: number[];
  color?: string;
  /** 固定峰值刻度;不传则自适应 */
  max?: number;
  bars?: number;
  height?: number;
  barWidth?: number;
}) {
  const { history, color = "var(--cyan)", max, bars = 6, height = 20, barWidth = 3.5 } = props;
  const vals = history.slice(-bars);
  while (vals.length < bars) vals.unshift(0);
  const peak = Math.max(max ?? 0, ...vals, 1e-9);
  return (
    <div style={{ display: "flex", alignItems: "flex-end", gap: 2.5, height }} aria-hidden>
      {vals.map((v, i) => {
        const n = Math.min(1, Math.max(0, v / peak));
        const gated = Math.max(0, n - NOISE_FLOOR) / (1 - NOISE_FLOOR);
        const compressed = Math.pow(gated, 0.7);
        const h = Math.max(2.5, compressed * height);
        return (
          <div
            key={i}
            style={{
              width: barWidth,
              height: h,
              borderRadius: 2,
              background: color,
              opacity: 0.4 + 0.6 * compressed,
              transition: `height 240ms ${CDS_EASING.EASE_OUT_EXPO}, opacity 240ms ease`,
            }}
          />
        );
      })}
    </div>
  );
}

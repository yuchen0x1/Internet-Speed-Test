// 网络质量评估纯函数(供 QualityPage 展示与单元测试复用)

/** 稳定性文案:抖动/负载比/丢包 3 项指标各命中 +1,score 范围 0-3 */
export function stabilityText(jitter: number, ratio: number | null, loss: number | null): string {
  let score = 0;
  if (jitter < 5) score += 1;
  if (ratio != null && ratio < 1.5) score += 1;
  if (loss != null && loss < 1) score += 1;
  return ["较差", "较弱", "中等", "良好"][score];
}

/** 稳定性文案 → 指标卡配色语气 */
export function stabilityTone(
  jitter: number,
  ratio: number | null,
  loss: number | null,
): "good" | "warn" | "bad" | "neutral" {
  const t = stabilityText(jitter, ratio, loss);
  return t === "良好" ? "good" : t === "中等" ? "warn" : "bad";
}

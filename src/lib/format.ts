// 格式化与错误嗅探的纯函数公共模块(单元测试直接覆盖)

/** Mbps → { value, unit } 分档显示 */
export function fmtSpeedMbps(mbps: number): { value: string; unit: string } {
  if (!isFinite(mbps) || mbps < 0) return { value: "--", unit: "Mbps" };
  if (mbps >= 1000) return { value: (mbps / 1000).toFixed(2), unit: "Gbps" };
  if (mbps >= 100) return { value: mbps.toFixed(1), unit: "Mbps" };
  return { value: mbps.toFixed(2), unit: "Mbps" };
}

/** 字节/秒 → "x.x KB/s" 等(状态栏/监控共用唯一实现) */
export function fmtBytesPerSec(bps: number): string {
  if (!isFinite(bps) || bps <= 0) return "0 B/s";
  const units = ["B/s", "KB/s", "MB/s", "GB/s"];
  let i = 0;
  let v = bps;
  while (v >= 1024 && i < units.length - 1) {
    v /= 1024;
    i++;
  }
  return `${v >= 100 ? v.toFixed(0) : v.toFixed(1)} ${units[i]}`;
}

/** 字节 → "x.xx MB" 等 */
export function fmtBytes(bytes: number): string {
  if (!isFinite(bytes) || bytes <= 0) return "0 B";
  const units = ["B", "KB", "MB", "GB", "TB"];
  let i = 0;
  let v = bytes;
  while (v >= 1024 && i < units.length - 1) {
    v /= 1024;
    i++;
  }
  return `${v >= 100 ? v.toFixed(0) : v.toFixed(2)} ${units[i]}`;
}

/** 导出/保存类操作被用户取消时不应弹错误提示 */
export function isCancelError(e: unknown): boolean {
  return String(e).includes("取消");
}

/**
 * 延迟曲线 x 轴口径(秒):样本序号 × 0.35 —— 测速页 / 质量页 / 历史详情回放三处共用,
 * 数值与原各自内联实现完全一致(样本上限 1200 条后序号不再增长,x 轴自然冻结)。
 */
export function latencyPoints(samples: [number, number][]): [number, number][] {
  return samples.map((s, i) => [i * 0.35, s[1]] as [number, number]);
}

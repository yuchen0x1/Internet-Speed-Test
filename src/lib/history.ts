import type { HistoryRow } from "../types";

/** 日历范围使用本地日期，避免 toISOString 在 UTC 换日时错一天。 */
export function historyDateRange(days: number, now = new Date()): { from: string; to: string } {
  const key = (d: Date) => `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, "0")}-${String(d.getDate()).padStart(2, "0")}`;
  const start = new Date(now);
  start.setDate(start.getDate() - (days - 1));
  return { from: key(start), to: key(now) };
}

/** 趋势始终按实际测试时间向前绘制，不依赖列表的排序方向。 */
export function recentCompleted(rows: HistoryRow[], limit = 40): HistoryRow[] {
  return rows.filter((r) => r.status === "completed")
    .sort((a, b) => a.created_at.localeCompare(b.created_at) || a.id - b.id)
    .slice(-limit);
}

// 测速历史热力图(GitHub 贡献图风格):按天聚合平均下载速度,
// 颜色深浅 = 速度四分位。数据全部来自真实历史记录。
import { useMemo } from "react";
import type { HistoryRow } from "../types";

function dayKey(s: string): string {
  // created_at 格式 "YYYY-MM-DD HH:mm:ss"
  return s.slice(0, 10);
}

export function HistoryHeatmap(props: { rows: HistoryRow[]; weeks?: number }) {
  const { rows, weeks = 14 } = props;
  const { cells, maxAvg, totalDays } = useMemo(() => {
    const byDay = new Map<string, { sum: number; n: number }>();
    for (const r of rows) {
      if (r.status !== "completed") continue;
      const k = dayKey(r.created_at);
      const e = byDay.get(k) ?? { sum: 0, n: 0 };
      e.sum += r.download_mbps;
      e.n += 1;
      byDay.set(k, e);
    }
    const days = [...byDay.entries()]
      .map(([k, v]) => ({ day: k, avg: v.sum / v.n }))
      .sort((a, b) => a.day.localeCompare(b.day));
    const maxAvg = Math.max(1, ...days.map((d) => d.avg));
    // 对齐到周列(列为周,行为星期一~日)
    const today = new Date();
    const end = new Date(today);
    const start = new Date(today);
    start.setDate(start.getDate() - (weeks * 7 - 1));
    const startPad = (start.getDay() + 6) % 7; // 周一=0
    start.setDate(start.getDate() - startPad);
    const cells: { key: string; avg: number | null; future: boolean }[] = [];
    const cur = new Date(start);
    while (cur <= end) {
      const k = `${cur.getFullYear()}-${String(cur.getMonth() + 1).padStart(2, "0")}-${String(cur.getDate()).padStart(2, "0")}`;
      const hit = days.find((d) => d.day === k);
      cells.push({ key: k, avg: hit ? hit.avg : null, future: cur > today });
      cur.setDate(cur.getDate() + 1);
    }
    return { cells, maxAvg, totalDays: days.length };
  }, [rows, weeks]);

  const lvl = (avg: number | null): string => {
    if (avg == null) return "rgba(122,176,255,.07)";
    const q = avg / maxAvg;
    if (q <= 0.25) return "rgba(34,211,238,.28)";
    if (q <= 0.5) return "rgba(34,211,238,.5)";
    if (q <= 0.75) return "rgba(34,211,238,.72)";
    return "rgba(34,211,238,1)";
  };

  return (
    <div>
      <div style={{ display: "grid", gridAutoFlow: "column", gridTemplateRows: "repeat(7, 11px)", gap: 3, overflowX: "auto", paddingBottom: 4 }}>
        {cells.map((c) => (
          <div
            key={c.key}
            title={c.avg != null ? `${c.key} · 平均下载 ${c.avg.toFixed(1)} Mbps` : `${c.key} · 无记录`}
            style={{
              width: 11, height: 11, borderRadius: 3,
              background: c.future ? "transparent" : lvl(c.avg),
              boxShadow: c.avg != null && c.avg / maxAvg > 0.75 ? "0 0 6px rgba(34,211,238,.55)" : undefined,
            }}
          />
        ))}
      </div>
      <div className="row mt8" style={{ gap: 6, fontSize: 11, color: "var(--text-faint)" }}>
        <span>{totalDays ? `共 ${totalDays} 个测试日` : "暂无完成记录"}</span>
        <div className="grow" />
        <span>低</span>
        {[0.07, 0.28, 0.5, 0.72, 1].map((o, i) => (
          <span key={i} style={{ width: 10, height: 10, borderRadius: 2.5, background: `rgba(34,211,238,${o})`, display: "inline-block" }} />
        ))}
        <span>高 · 峰值日均 {maxAvg > 1 ? maxAvg.toFixed(0) : "—"} Mbps</span>
      </div>
    </div>
  );
}

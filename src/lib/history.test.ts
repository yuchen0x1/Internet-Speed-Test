import { describe, expect, it } from "vitest";
import { historyDateRange, recentCompleted } from "./history";
import type { HistoryRow } from "../types";

describe("history calendar ranges", () => {
  it("includes today and crosses month/year boundaries in local time", () => {
    const now = new Date(2026, 0, 2, 0, 15);
    expect(historyDateRange(1, now)).toEqual({ from: "2026-01-02", to: "2026-01-02" });
    expect(historyDateRange(7, now)).toEqual({ from: "2025-12-27", to: "2026-01-02" });
    expect(now.getDate()).toBe(2);
  });
});

describe("history trend chronology", () => {
  it("gives the same recent successful tests for either list order without mutating rows", () => {
    const rows = [
      { id: 1, created_at: "2026-11-06 11:00:00", status: "completed" },
      { id: 2, created_at: "2026-11-07 11:00:00", status: "failed" },
      { id: 3, created_at: "2026-11-06 11:00:00", status: "completed" },
      { id: 4, created_at: "2026-11-08 11:00:00", status: "completed" },
    ] as HistoryRow[];
    expect(recentCompleted(rows, 2).map((r) => r.id)).toEqual([3, 4]);
    expect(recentCompleted([...rows].reverse(), 2)).toEqual(recentCompleted(rows, 2));
    expect(rows.map((r) => r.id)).toEqual([1, 2, 3, 4]);
  });
});

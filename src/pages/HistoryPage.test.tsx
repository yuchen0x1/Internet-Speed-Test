// @vitest-environment jsdom
import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { HistoryPage } from "./HistoryPage";
import { api } from "../api";
import { useHistoryUi } from "../store/ui";
import { useApp } from "../store/app";
import type { HistoryRow } from "../types";

vi.mock("../api", () => ({
  api: { historyList: vi.fn(), historySummary: vi.fn(), historyExport: vi.fn() },
}));
vi.mock("../components/LineChart", () => ({ LineChart: () => null }));
vi.mock("../components/HistoryHeatmap", () => ({ HistoryHeatmap: () => null }));

// DOM 交互测试使用隔离 IPC fixture，不会读写用户数据库。
const row = (id: number): HistoryRow => ({
  id, created_at: `2026-11-07 10:00:${String(id).padStart(2, "0")}`,
  status: "completed", download_mbps: 10, upload_mbps: null, idle_ping_ms: 20,
  jitter_ms: 1, loaded_down_ms: null, loaded_up_ms: null, packet_loss_percent: null,
  server_name: "Fixture", server_provider: "custom", server_url: "https://example.test",
  network_type: "ethernet", interface_name: "", public_ip: null, isp: null,
});
let root: Root;
let host: HTMLDivElement;
const toast = vi.fn();
const button = (text: string) => [...host.querySelectorAll("button")].find((b) => b.textContent?.includes(text))!;

beforeEach(() => {
  Object.assign(globalThis, { IS_REACT_ACT_ENVIRONMENT: true });
  vi.clearAllMocks();
  useHistoryUi.setState({ from: "", to: "", desc: true, selected: [] });
  useApp.setState({ toast });
  vi.mocked(api.historyList).mockResolvedValue([row(1), row(2)]);
  vi.mocked(api.historySummary).mockResolvedValue({
    total: 127, completed: 125, failed: 1, cancelled: 1,
    avg_download_mbps: 62, avg_upload_mbps: null, avg_ping_ms: 20, best_download_mbps: 124,
  });
  vi.mocked(api.historyExport).mockResolvedValue({ path: "fixture.csv", total: 127, exported: 127 });
  host = document.createElement("div");
  document.body.append(host);
  root = createRoot(host);
});
afterEach(async () => {
  await act(async () => root.unmount());
  host.remove();
});

describe("history interactions", () => {
  it("shows full-range aggregates and exports the active range and order", async () => {
    useHistoryUi.setState({ from: "2026-11-07", to: "2026-11-07", desc: false });
    await act(async () => root.render(<HistoryPage />));
    expect(host.textContent).toContain("完成 125 · 失败 1 · 取消 1");
    expect(host.textContent).toContain("62.0");
    expect(host.textContent).toContain("仅有效上传结果");
    expect(api.historySummary).toHaveBeenCalledWith({ from: "2026-11-07", to: "2026-11-07" });
    await act(async () => button("CSV").click());
    expect(api.historyExport).toHaveBeenCalledWith("csv", { from: "2026-11-07", to: "2026-11-07", desc: false });
  });

  it("reports load failures, retries, and clears invalid ranges without querying", async () => {
    vi.mocked(api.historyList).mockRejectedValueOnce(new Error("unavailable"));
    await act(async () => root.render(<HistoryPage />));
    expect(host.querySelector('[role="alert"]')?.textContent).toContain("历史加载失败");
    await act(async () => button("重试加载").click());
    expect(host.querySelector('[role="alert"]')).toBeNull();
    const calls = vi.mocked(api.historyList).mock.calls.length;
    await act(async () => useHistoryUi.getState().setRange("2026-11-08", "2026-11-07"));
    expect(host.textContent).toContain("起始日期不能晚于结束日期");
    expect(api.historyList).toHaveBeenCalledTimes(calls);
    expect(button("CSV").disabled).toBe(true);
    await act(async () => button("全部日期").click());
    expect(api.historyList).toHaveBeenCalledTimes(calls + 1);
  });

  it("drops late responses after switching ranges", async () => {
    let resolveOld!: (rows: HistoryRow[]) => void;
    vi.mocked(api.historyList).mockImplementationOnce(() => new Promise((resolve) => { resolveOld = resolve; }));
    await act(async () => root.render(<HistoryPage />));
    await act(async () => useHistoryUi.getState().setRange("2026-11-07", "2026-11-07"));
    await act(async () => resolveOld([{ ...row(3), server_name: "stale-response" }]));
    expect(host.textContent).not.toContain("stale-response");
    expect(host.querySelectorAll("tbody tr")).toHaveLength(2);
  });

  it("loads more with a cursor, deduplicates rows, and keeps full-range averages", async () => {
    vi.mocked(api.historyList)
      .mockResolvedValueOnce(Array.from({ length: 101 }, (_, i) => row(i + 1)))
      .mockResolvedValueOnce([row(100), row(101), row(102)]);
    await act(async () => root.render(<HistoryPage />));
    expect(host.querySelectorAll("tbody tr")).toHaveLength(100);
    await act(async () => button("加载更多").click());
    expect(api.historyList).toHaveBeenLastCalledWith({
      from: undefined, to: undefined, desc: true, limit: 101, cursorId: 100,
    });
    expect(host.querySelectorAll("tbody tr")).toHaveLength(102);
    expect(host.textContent).toContain("62.0");
    expect(api.historySummary).toHaveBeenCalledTimes(1);
  });
});

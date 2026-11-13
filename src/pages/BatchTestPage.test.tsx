// @vitest-environment jsdom
import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { BatchTestPage } from "./BatchTestPage";
import { useBatch } from "../store/batch";
import { useApp } from "../store/app";
import { api } from "../api";
vi.mock("../api", () => ({ api: { batchtestExport: vi.fn() }, onBatchUpdate: vi.fn(), errMsg: String }));
let root: Root; let host: HTMLDivElement;
beforeEach(() => {
  Object.assign(globalThis, { IS_REACT_ACT_ENVIRONMENT: true });
  useApp.setState({ toast: vi.fn() });
  useBatch.setState({ bound: true, busy: false, error: null, bind: async () => undefined,
    running: false, total: 20, doneCount: 3, activeTargets: ["localhost", "queued.test"],
    snapshot: { run_id: 7, started_at: "2026-11-13T12:00:00+08:00", revision: 4, running: false, cancelled: true, count: 10, targets: ["localhost", "queued.test"], results: [] },
    results: [{ target: "localhost", sent: 3, recv: 2, loss_percent: 100/3, min_ms: 1, max_ms: 2, avg_ms: 1.5, latest_ms: 2, samples: [1, null, 2] }],
  });
  host = document.createElement("div"); document.body.append(host); root = createRoot(host);
});
afterEach(async () => { await act(async () => root.unmount()); host.remove(); });
it("shows partial packet progress, unknown queued results and gaps across timeouts", async () => {
  await act(async () => root.render(<BatchTestPage />));
  expect(host.textContent).toContain("已停止 · 保留部分结果");
  expect(host.textContent).toContain("3/20 包");
  expect(host.querySelector('[role="progressbar"]')?.getAttribute("aria-valuenow")).toBe("3");
  const queued = [...host.querySelectorAll("tbody tr")].find((r) => r.textContent?.includes("queued.test"))!;
  expect(queued.textContent).toContain("未探测");
  expect(queued.textContent).not.toContain("0%");
  expect(host.querySelectorAll("polyline")).toHaveLength(0);
  expect(host.querySelector('label[for="batch-targets"]')).not.toBeNull();
});
it("exports the selected backend run rather than sending editable results", async () => {
  vi.mocked(api.batchtestExport).mockResolvedValueOnce("fixture.csv");
  await act(async () => root.render(<BatchTestPage />));
  const exportButton = [...host.querySelectorAll("button")].find((b) => b.textContent === "导出 CSV")!;
  await act(async () => exportButton.click());
  expect(api.batchtestExport).toHaveBeenCalledWith(7, "csv");
});

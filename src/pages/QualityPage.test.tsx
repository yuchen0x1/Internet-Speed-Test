// @vitest-environment jsdom
import { act } from "react";
import { createRoot } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { QualityPage } from "./QualityPage";
import { useTest } from "../store/test";
import { RadarChart } from "../components/RadarChart";
import type { TestResult } from "../types";
vi.mock("../api", () => ({ api: {} }));
vi.mock("../components/RadarChart", () => ({ RadarChart: vi.fn(() => null) }));
vi.mock("../components/LineChart", () => ({ LineChart: () => null }));
let host: HTMLDivElement;
let root: ReturnType<typeof createRoot>;
beforeEach(() => {
  Object.assign(globalThis, { IS_REACT_ACT_ENVIRONMENT: true });
  host = document.createElement("div"); document.body.append(host); root = createRoot(host);
  vi.clearAllMocks();
});
afterEach(async () => { await act(async () => root.unmount()); host.remove(); });
const result = {
  status: "completed", quality: { score: 100, rating: "优秀", notes: [] },
  idle_ping_ms: 10, jitter_ms: 1, packet_loss_percent: null, loaded_down_ms: null,
  loaded_up_ms: null, upload_mbps: null, download_mbps: 10, http_failures: 0,
  ping_min_ms: 10, ping_max_ms: 10, latency_samples: [], down_samples: [], up_samples: [], conditions: [],
} as unknown as TestResult;
describe("quality assessment boundaries", () => {
  it("does not show an old excellent rating for an immediately cancelled test", async () => {
    useTest.setState({ result: { ...result, status: "cancelled" } });
    await act(async () => root.render(<QualityPage />));
    expect(host.textContent).toContain("未生成完整质量评级");
    expect(host.textContent).not.toContain("优秀");
    expect(RadarChart).not.toHaveBeenCalled();
  });
  it("passes unknown packet loss and upload through to the chart", async () => {
    useTest.setState({ result });
    await act(async () => root.render(<QualityPage />));
    const metrics = vi.mocked(RadarChart).mock.calls.at(-1)![0].metrics;
    expect(metrics.find(m => m.label === "丢包")?.value).toBeNull();
    expect(metrics.find(m => m.label === "上传")?.value).toBeNull();
  });
});

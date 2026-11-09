import { describe, expect, it, vi } from "vitest";
import { appendSpeedSample, MAX_SAMPLES } from "./test";

// 隔离 Tauri IPC:只测纯函数,不触发任何事件监听
vi.mock("../api", () => ({
  api: {},
  onEngineEvent: vi.fn(),
  onServersProbed: vi.fn(),
  onMonitorTick: vi.fn(),
  onNetStatus: vi.fn(),
}));

describe("appendSpeedSample(x 轴采样计算)", () => {
  it("首条样本 x 轴从 0 开始", () => {
    expect(appendSpeedSample([], 100)).toEqual([[0, 100]]);
  });

  it("x 轴按 0.2s 粒度递增,值原样保留", () => {
    let s: [number, number][] = [];
    s = appendSpeedSample(s, 10);
    s = appendSpeedSample(s, 20);
    s = appendSpeedSample(s, 30);
    expect(s.map((p) => p[0])).toEqual([0, 0.2, 0.4]);
    expect(s.map((p) => p[1])).toEqual([10, 20, 30]);
  });

  it("超出 MAX_SAMPLES 上限时裁掉最旧样本、保留最新值", () => {
    let s: [number, number][] = [];
    for (let i = 0; i < MAX_SAMPLES + 10; i++) s = appendSpeedSample(s, i);
    expect(s.length).toBeLessThanOrEqual(MAX_SAMPLES + 1);
    expect(s[s.length - 1][1]).toBe(MAX_SAMPLES + 9);
    expect(s[0][1]).toBe(9);
  });
});

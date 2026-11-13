// @vitest-environment jsdom
import { afterEach, describe, expect, it, vi } from "vitest";
import { appendSpeedSample, MAX_SAMPLES, useTest } from "./test";
import { api, onEngineEvent, onServersProbed } from "../api";
import { useApp } from "./app";
import type { EngineEvent, TestResult } from "../types";

// 隔离 Tauri IPC:只测纯函数,不触发任何事件监听
vi.mock("../api", () => ({
  api: { speedtestRunning: vi.fn(), speedtestPhase: vi.fn() },
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
    expect(s.length).toBe(MAX_SAMPLES);
    expect(s[s.length - 1][1]).toBe(MAX_SAMPLES + 9);
    expect(s[0][1]).toBe(10);
    for (let i = 1; i < s.length; i++) expect(s[i][0]).toBeGreaterThan(s[i - 1][0]);
  });
});

describe("engine events", () => {
  afterEach(() => {
    useTest.getState().reset();
    useTest.setState({ bound: false });
    vi.useRealTimers();
  });

  it("keeps elapsed time through phase changes and handles a failed final result", async () => {
    vi.useFakeTimers();
    let emit!: (event: EngineEvent) => void;
    vi.mocked(onEngineEvent).mockImplementationOnce(async (handler) => {
      emit = handler;
      return () => undefined;
    });
    vi.mocked(onServersProbed).mockResolvedValueOnce(() => undefined);
    const toast = vi.fn();
    useApp.setState({ toast });
    await useTest.getState().bind();
    emit({ type: "phase", phase: "preparing", message: "准备" });
    vi.advanceTimersByTime(1000);
    const before = useTest.getState().elapsedMs;
    emit({ type: "phase", phase: "download", message: "下载" });
    vi.advanceTimersByTime(1000);
    expect(useTest.getState().elapsedMs).toBeGreaterThanOrEqual(before + 1000);
    emit({ type: "finished", result: {
      status: "failed", duration_ms: 2000, error: "offline", history_save_error: "保存失败",
    } as TestResult });
    expect(useTest.getState()).toMatchObject({ running: false, progress: 0, message: "测速失败", error: "offline" });
    expect(toast).toHaveBeenCalledWith("warn", "保存失败");
    expect(vi.getTimerCount()).toBe(0);
  });

  it("cleans up partial subscriptions and retries without duplicate listeners", async () => {
    const off = vi.fn();
    vi.mocked(onEngineEvent).mockReset().mockResolvedValue(off);
    vi.mocked(onServersProbed).mockReset().mockRejectedValueOnce(new Error("not ready"))
      .mockResolvedValueOnce(() => undefined);
    await expect(useTest.getState().bind()).rejects.toThrow("not ready");
    expect(off).toHaveBeenCalledOnce();
    expect(useTest.getState().bound).toBe(false);
    await Promise.all([useTest.getState().bind(), useTest.getState().bind()]);
    expect(onEngineEvent).toHaveBeenCalledTimes(2);
    expect(useTest.getState().bound).toBe(true);
  });

  it("does not apply a stale running response over a newer finished event", async () => {
    let emit!: (event: EngineEvent) => void;
    vi.mocked(onEngineEvent).mockImplementationOnce(async (handler) => { emit = handler; return () => undefined; });
    vi.mocked(onServersProbed).mockResolvedValueOnce(() => undefined);
    await useTest.getState().bind();
    let finishQuery!: (running: boolean) => void;
    vi.mocked(api.speedtestRunning).mockImplementationOnce(() => new Promise((resolve) => { finishQuery = resolve; }));
    vi.mocked(api.speedtestPhase).mockResolvedValueOnce("download");
    const sync = useTest.getState().syncRunning();
    emit({ type: "finished", result: { status: "completed", error: null, duration_ms: 1500 } as TestResult });
    finishQuery(true);
    await sync;
    expect(useTest.getState()).toMatchObject({ running: false, message: "测速完成", elapsedMs: 1500 });
  });

  it("clears previous results when recovering a different background test", async () => {
    vi.useFakeTimers();
    useTest.setState({ result: { status: "completed" } as TestResult, elapsedMs: 10000, downSamples: [[0, 99]] });
    vi.mocked(api.speedtestRunning).mockResolvedValueOnce(true);
    vi.mocked(api.speedtestPhase).mockResolvedValueOnce("download");
    await useTest.getState().syncRunning();
    expect(useTest.getState()).toMatchObject({ running: true, result: null, elapsedMs: 0, downSamples: [] });
  });
});

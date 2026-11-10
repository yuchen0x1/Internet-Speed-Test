// @vitest-environment jsdom
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { api, onMonitorTick } from "../api";
import { useApp, useMonitor } from "./app";
import type { AppSettings } from "../types";

// 隔离 Tauri IPC:仅测 store 的设置更新逻辑,零网络
vi.mock("../api", () => ({
  api: {
    setSettings: vi.fn(),
    getSettings: vi.fn(),
    appInfo: vi.fn(),
    netStatus: vi.fn(),
    monitorPaused: vi.fn(),
    monitorSetPaused: vi.fn(),
    monitorSetEnabled: vi.fn(),
  },
  onMonitorTick: vi.fn(),
  onNetStatus: vi.fn(),
}));

const baseSettings: AppSettings = {
  theme: "dark",
  animations: true,
  anim_intensity: 1,
  refresh_interval_ms: 1000,
  node_mode: "auto",
  default_node_url: "https://example.com/",
  test_duration_secs: 10,
  precision_mode: false,
  down_concurrency: 6,
  up_concurrency: 4,
  autosave_history: true,
  monitor_enabled: false,
  show_traffic: false,
  autostart: false,
  auto_test_enabled: false,
  auto_test_interval_minutes: 30,
  notify_events: true,
  alert_enabled: false,
  alert_threshold_ms: 150,
  alert_target: "223.5.5.5",
  retention_days: 0,
  prometheus_enabled: false,
  prometheus_port: 9100,
  show_public_ip: true,
  save_diag_history: true,
  close_to_tray: true,
  custom_nodes: [],
};

describe("updateSettings", () => {
  beforeEach(() => {
    vi.useFakeTimers();
    vi.mocked(api.setSettings).mockReset().mockResolvedValue(undefined);
    useApp.setState({ settings: { ...baseSettings }, toasts: [] });
  });

  afterEach(() => {
    vi.useRealTimers();
  });

  it("保存成功:返回 true、合并补丁、不弹错误", async () => {
    const ok = await useApp.getState().updateSettings({ autostart: true });
    expect(ok).toBe(true);
    expect(useApp.getState().settings?.autostart).toBe(true);
    expect(api.setSettings).toHaveBeenCalledTimes(1);
    expect(useApp.getState().toasts).toHaveLength(0);
  });

  it("保存失败:返回 false、回滚本地设置并弹错误提示", async () => {
    vi.mocked(api.setSettings).mockRejectedValueOnce(new Error("disk full"));
    const ok = await useApp.getState().updateSettings({ autostart: true });
    expect(ok).toBe(false);
    expect(useApp.getState().settings?.autostart).toBe(false);
    expect(useApp.getState().toasts.some((t) => t.kind === "error")).toBe(true);
  });

  it("并发设置保存按最新完整状态串行写入", async () => {
    let finishFirst!: () => void;
    vi.mocked(api.setSettings)
      .mockImplementationOnce(() => new Promise<void>((resolve) => { finishFirst = resolve; }))
      .mockResolvedValueOnce(undefined);

    const first = useApp.getState().updateSettings({ autostart: true });
    const second = useApp.getState().updateSettings({ monitor_enabled: true });
    expect(api.setSettings).toHaveBeenCalledTimes(0);

    await Promise.resolve();
    expect(api.setSettings).toHaveBeenCalledTimes(1);
    expect(vi.mocked(api.setSettings).mock.calls[0][0]).toMatchObject({
      autostart: true,
      monitor_enabled: true,
    });

    finishFirst();
    await expect(Promise.all([first, second])).resolves.toEqual([true, true]);
    expect(api.setSettings).toHaveBeenCalledTimes(2);
    expect(vi.mocked(api.setSettings).mock.calls[1][0]).toMatchObject({
      autostart: true,
      monitor_enabled: true,
    });
  });

  it("persist=false:不调用后端也返回 true", async () => {
    const ok = await useApp.getState().updateSettings({ test_duration_secs: 20 }, false);
    expect(ok).toBe(true);
    expect(api.setSettings).not.toHaveBeenCalled();
    expect(useApp.getState().settings?.test_duration_secs).toBe(20);
  });

  it("settings 未加载时返回 false 且不做任何改动", async () => {
    useApp.setState({ settings: null });
    const ok = await useApp.getState().updateSettings({ autostart: true });
    expect(ok).toBe(false);
    expect(useApp.getState().settings).toBeNull();
  });
});

describe("monitor binding", () => {
  beforeEach(() => {
    useMonitor.setState({ paused: false, bound: false, buffer: [], latest: null });
    vi.mocked(api.monitorPaused).mockReset().mockResolvedValue(false);
    vi.mocked(onMonitorTick).mockReset().mockResolvedValue(() => undefined);
  });

  it("restores paused state from backend when binding", async () => {
    vi.mocked(api.monitorPaused).mockResolvedValueOnce(true);

    await useMonitor.getState().bind();

    expect(useMonitor.getState().paused).toBe(true);
    expect(api.monitorPaused).toHaveBeenCalledOnce();
    expect(onMonitorTick).toHaveBeenCalledOnce();
  });

  it("continues binding if paused-state query fails", async () => {
    vi.mocked(api.monitorPaused).mockRejectedValueOnce(new Error("unavailable"));

    await useMonitor.getState().bind();

    expect(useMonitor.getState().paused).toBe(false);
    expect(onMonitorTick).toHaveBeenCalledOnce();
  });
});

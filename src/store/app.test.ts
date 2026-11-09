// @vitest-environment jsdom
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { api } from "../api";
import { useApp } from "./app";
import type { AppSettings } from "../types";

// 隔离 Tauri IPC:仅测 store 的设置更新逻辑,零网络
vi.mock("../api", () => ({
  api: {
    setSettings: vi.fn(),
    getSettings: vi.fn(),
    appInfo: vi.fn(),
    netStatus: vi.fn(),
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

  it("保存失败:返回 false,本地乐观更新保留并弹错误提示", async () => {
    vi.mocked(api.setSettings).mockRejectedValueOnce(new Error("disk full"));
    const ok = await useApp.getState().updateSettings({ autostart: true });
    expect(ok).toBe(false);
    // 本地状态仍为乐观更新值
    expect(useApp.getState().settings?.autostart).toBe(true);
    expect(useApp.getState().toasts.some((t) => t.kind === "error")).toBe(true);
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

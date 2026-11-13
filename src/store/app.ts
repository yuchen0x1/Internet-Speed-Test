// 应用级 store:设置、网络状态、监控实时数据、UI(页面/通知)

import { create } from "zustand";
import { api, onMonitorTick, onNetStatus } from "../api";
import type { AppInfo, AppSettings, MonitorTick, NetStatus } from "../types";

export type PageId =
  | "speedtest"
  | "quality"
  | "diagnostics"
  | "history"
  | "servers"
  | "batchtest"
  | "monitor"
  | "settings";

interface AppStore {
  booted: boolean;
  appInfo: AppInfo | null;
  settings: AppSettings | null;
  net: NetStatus;
  page: PageId;
  toasts: { id: number; kind: "success" | "error" | "warn" | "info"; text: string }[];
  settingsDirty: boolean;

  boot: () => Promise<void>;
  setPage: (p: PageId) => void;
  updateSettings: (patch: Partial<AppSettings>, persist?: boolean) => Promise<boolean>;
  toast: (kind: "success" | "error" | "warn" | "info", text: string) => void;
  dismissToast: (id: number) => void;
  applyTheme: () => void;
}

let toastId = 1;
let listenersBound = false;
let settingsSaveQueue: Promise<void> = Promise.resolve();

export const useApp = create<AppStore>((set, get) => ({
  booted: false,
  appInfo: null,
  settings: null,
  net: { connected: false, kind: "none", iface: "", link_mbps: null, ipv4: null, has_ipv6: false },
  page: "speedtest",
  toasts: [],
  settingsDirty: false,

  boot: async () => {
    try {
      const [info, settings, net] = await Promise.all([
        api.appInfo(),
        api.getSettings(),
        api.netStatus(),
      ]);
      set({ appInfo: info, settings, net, booted: true });
      get().applyTheme();
      if (!listenersBound) {
        listenersBound = true;
        onNetStatus((s) => set({ net: s }));
      }
    } catch (e) {
      console.error("启动失败", e);
      set({ booted: true });
    }
  },

  setPage: (p) => set({ page: p }),

  // 保存成功后提交 UI 状态。每个排队操作只合并自己的补丁，
  // 避免前一个请求提前保存后续补丁，或失败回滚到从未保存过的值。
  updateSettings: async (patch, persist = true) => {
    if (!get().settings) return false;
    if (persist) {
      const save = settingsSaveQueue.then(async () => {
        const latest = get().settings;
        if (!latest) return false;
        const next = { ...latest, ...patch };
        try {
          const saved = await api.setSettings(next);
          set({ settings: saved });
          get().applyTheme();
          return true;
        } catch (e) {
          get().toast("error", `设置保存失败:${String(e)}`);
          return false;
        }
      });
      settingsSaveQueue = save.then(() => undefined);
      return save;
    }
    set((state) => ({ settings: state.settings ? { ...state.settings, ...patch } : null }));
    get().applyTheme();
    return true;
  },

  toast: (kind, text) => {
    const id = toastId++;
    set((s) => ({ toasts: [...s.toasts.slice(-4), { id, kind, text }] }));
    window.setTimeout(() => get().dismissToast(id), kind === "error" ? 6000 : 3200);
  },

  dismissToast: (id) => set((s) => ({ toasts: s.toasts.filter((t) => t.id !== id) })),

  applyTheme: () => {
    const s = get().settings;
    if (!s) return;
    const root = document.documentElement;
    const theme =
      s.theme === "system"
        ? window.matchMedia("(prefers-color-scheme: light)").matches
          ? "light"
          : "dark"
        : s.theme;
    root.classList.toggle("theme-dark", theme === "dark");
    root.classList.toggle("theme-light", theme === "light");
    root.classList.toggle("no-anim", !s.animations);
    root.style.setProperty("--anim-scale", String(s.animations ? s.anim_intensity : 0));
  },
}));

/** 节点专用命令与普通设置保存共用队列，完成后同步后端权威状态。 */
export function runSettingsMutation(action: () => Promise<unknown>): Promise<void> {
  const mutation = settingsSaveQueue.then(async () => {
    await action();
    const settings = await api.getSettings();
    useApp.setState({ settings });
    useApp.getState().applyTheme();
  });
  settingsSaveQueue = mutation.catch(() => undefined);
  return mutation;
}

// ------------------------------------------------------------
// 监控实时数据(ring buffer,1s 粒度,保留 1h)
// ------------------------------------------------------------

interface MonitorStore {
  buffer: MonitorTick[];
  latest: MonitorTick | null;
  paused: boolean;
  bound: boolean;
  bind: () => Promise<void>;
  setPaused: (p: boolean) => Promise<void>;
  backfill: (windowSecs: number) => Promise<void>;
  clear: () => void;
}

const MAX_POINTS = 3600;
let monitorBinding: Promise<void> | null = null;
let monitorPauseQueue: Promise<void> = Promise.resolve();

export const useMonitor = create<MonitorStore>((set, get) => ({
  buffer: [],
  latest: null,
  paused: false,
  bound: false,

  bind: async () => {
    if (get().bound) return;
    if (monitorBinding) return monitorBinding;
    monitorBinding = (async () => {
      try {
        set({ paused: await api.monitorPaused() });
      } catch {
        // 状态查询不可用时保留本地默认值,事件绑定仍需继续。
      }
      await onMonitorTick((tick) => {
        set((s) => {
          const buffer = [...s.buffer, tick];
          if (buffer.length > MAX_POINTS) buffer.splice(0, buffer.length - MAX_POINTS);
          return { buffer, latest: tick };
        });
      });
      set({ bound: true });
    })().finally(() => { monitorBinding = null; });
    return monitorBinding;
  },

  setPaused: async (p) => {
    const save = monitorPauseQueue.then(async () => {
      await api.monitorSetPaused(p);
      set({ paused: p });
    });
    monitorPauseQueue = save.catch(() => undefined);
    return save;
  },

  backfill: async (windowSecs) => {
    try {
      const ticks = await api.monitorHistory(windowSecs);
      set((state) => {
        // 回填可能晚于实时事件返回。按时间合并，不能用旧响应覆盖刚收到的点。
        const byTime = new Map([...ticks, ...state.buffer].map((tick) => [tick.ts_ms, tick]));
        const buffer = [...byTime.values()].sort((a, b) => a.ts_ms - b.ts_ms).slice(-MAX_POINTS);
        return { buffer, latest: buffer.at(-1) ?? state.latest };
      });
    } catch {
      /* 忽略回填失败 */
    }
  },

  clear: () => set({ buffer: [], latest: null }),
}));


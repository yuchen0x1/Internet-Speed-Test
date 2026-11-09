// 应用级 store:设置、网络状态、监控实时数据、UI(页面/通知)

import { create } from "zustand";
import { api, onMonitorTick, onNetStatus } from "../api";
import type { AppSettings, MonitorTick, NetStatus } from "../types";

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
  appInfo: { name: string; version: string; os: string; data_dir: string } | null;
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

  // 返回是否保存成功:乐观更新本地状态,persist 失败时内部弹错误提示并返回 false,
  // 调用方据此决定是否展示成功提示(如设置页开机自启)。
  updateSettings: async (patch, persist = true) => {
    const cur = get().settings;
    if (!cur) return false;
    const next = { ...cur, ...patch };
    set({ settings: next });
    get().applyTheme();
    if (persist) {
      try {
        await api.setSettings(next);
      } catch (e) {
        get().toast("error", `设置保存失败:${String(e)}`);
        return false;
      }
    }
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

export const useMonitor = create<MonitorStore>((set, get) => ({
  buffer: [],
  latest: null,
  paused: false,
  bound: false,

  bind: async () => {
    if (get().bound) return;
    set({ bound: true });
    await onMonitorTick((tick) => {
      set((s) => {
        const buffer = [...s.buffer, tick];
        if (buffer.length > MAX_POINTS) buffer.splice(0, buffer.length - MAX_POINTS);
        return { buffer, latest: tick };
      });
    });
  },

  setPaused: async (p) => {
    set({ paused: p });
    await api.monitorSetPaused(p);
  },

  backfill: async (windowSecs) => {
    try {
      const ticks = await api.monitorHistory(windowSecs);
      if (ticks.length > get().buffer.length) {
        set({ buffer: ticks.slice(-MAX_POINTS), latest: ticks[ticks.length - 1] ?? null });
      }
    } catch {
      /* 忽略回填失败 */
    }
  },

  clear: () => set({ buffer: [], latest: null }),
}));


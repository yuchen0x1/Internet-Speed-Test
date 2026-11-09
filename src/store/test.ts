// 测速实时状态:由引擎事件驱动的状态机

import { create } from "zustand";
import { api, onEngineEvent, onServersProbed } from "../api";
import { useApp } from "./app";
import type { Phase, ProbeResult, ServerNodeInfo, TestResult } from "../types";

// 实时样本上限(防止长时测试内存膨胀)
export const MAX_SAMPLES = 1200;

/**
 * 实时采样追加:x 轴 = 样本条数 × 0.2s(200ms 粒度),超出上限裁掉最旧样本。
 * 纯函数,便于单元测试。
 */
export function appendSpeedSample(samples: [number, number][], mbps: number): [number, number][] {
  return [...samples.slice(-MAX_SAMPLES), [samples.length * 0.2, mbps]];
}

interface TestStore {
  phase: Phase;
  message: string;
  progress: number;
  running: boolean;
  elapsedMs: number;
  node: ServerNodeInfo | null;
  liveDown: number;
  liveUp: number;
  downSamples: [number, number][];
  upSamples: [number, number][];
  latencySamples: [number, number][];
  loadedDown: number[];
  loadedUp: number[];
  downloadedMb: number;
  uploadedMb: number;
  result: TestResult | null;
  error: string | null;
  bound: boolean;

  bind: () => Promise<void>;
  reset: () => void;
}

let elapsedTimer: number | null = null;

function stopTimer() {
  if (elapsedTimer != null) {
    window.clearInterval(elapsedTimer);
    elapsedTimer = null;
  }
}

export const useTest = create<TestStore>((set, get) => ({
  phase: "idle",
  message: "",
  progress: 0,
  running: false,
  elapsedMs: 0,
  node: null,
  liveDown: 0,
  liveUp: 0,
  downSamples: [],
  upSamples: [],
  latencySamples: [],
  loadedDown: [],
  loadedUp: [],
  downloadedMb: 0,
  uploadedMb: 0,
  result: null,
  error: null,
  bound: false,

  bind: async () => {
    if (get().bound) return;
    set({ bound: true });
    await onEngineEvent((ev) => {
      switch (ev.type) {
        case "phase": {
          const running = ev.phase !== "finished" && ev.phase !== "idle";
          set({
            phase: ev.phase as Phase,
            message: ev.message,
            running,
            progress: 0,
            error: null,
            ...(ev.phase === "preparing"
              ? { downSamples: [], upSamples: [], latencySamples: [], loadedDown: [], loadedUp: [], downloadedMb: 0, uploadedMb: 0, liveDown: 0, liveUp: 0, result: null }
              : {}),
          });
          stopTimer();
          if (running) {
            const t0 = Date.now();
            elapsedTimer = window.setInterval(
              () => set((s) => ({ elapsedMs: s.running ? Date.now() - t0 : s.elapsedMs })),
              200,
            );
          }
          break;
        }
        case "server":
          set({ node: ev.node });
          break;
        case "latency":
          set((s) => ({
            latencySamples: [...s.latencySamples.slice(-MAX_SAMPLES), [Date.now() % 1e7, ev.rtt_ms]],
          }));
          break;
        case "speed":
          if (ev.direction === "download") {
            set((s) => ({
              liveDown: ev.mbps,
              downloadedMb: ev.cumulative_mb,
              downSamples: appendSpeedSample(s.downSamples, ev.mbps),
            }));
          } else {
            set((s) => ({
              liveUp: ev.mbps,
              uploadedMb: ev.cumulative_mb,
              upSamples: appendSpeedSample(s.upSamples, ev.mbps),
            }));
          }
          break;
        case "loaded_latency":
          if (ev.direction === "download") {
            set((s) => ({ loadedDown: [...s.loadedDown.slice(-200), ev.rtt_ms] }));
          } else {
            set((s) => ({ loadedUp: [...s.loadedUp.slice(-200), ev.rtt_ms] }));
          }
          break;
        case "progress":
          set({ progress: ev.percent });
          break;
        case "finished":
          stopTimer();
          set({
            running: false,
            result: ev.result,
            phase: "finished",
            progress: 100,
            liveDown: 0,
            liveUp: 0,
            elapsedMs: ev.result.duration_ms,
            error: ev.result.error,
          });
          break;
        case "failed":
          stopTimer();
          set({ running: false, phase: "finished", error: ev.error, progress: 0 });
          break;
        case "cancelled":
          stopTimer();
          set({ running: false, phase: "finished", progress: 0, message: "已取消" });
          break;
      }
    });
    await onServersProbed((probes: ProbeResult[]) => {
      useServers.getState().applyProbe(probes);
    });
  },

  reset: () =>
    set({
      phase: "idle",
      message: "",
      progress: 0,
      running: false,
      elapsedMs: 0,
      liveDown: 0,
      liveUp: 0,
      downloadedMb: 0,
      uploadedMb: 0,
      downSamples: [],
      upSamples: [],
      latencySamples: [],
      loadedDown: [],
      loadedUp: [],
      result: null,
      error: null,
    }),
}));

// ------------------------------------------------------------
// 节点列表 store
// ------------------------------------------------------------

interface ServersStore {
  nodes: ServerNodeInfo[];
  loading: boolean;
  probing: boolean;
  loaded: boolean;
  load: (force?: boolean) => Promise<void>;
  refresh: () => Promise<void>;
  applyProbe: (probes: ProbeResult[]) => void;
}

export const useServers = create<ServersStore>((set, get) => ({
  nodes: [],
  loading: false,
  probing: false,
  loaded: false,

  load: async (force = false) => {
    if (get().loading || (get().loaded && !force)) return;
    set({ loading: true });
    try {
      const nodes = await api.serversList();
      set({ nodes, loaded: true });
    } finally {
      set({ loading: false });
    }
  },

  refresh: async () => {
    if (get().probing) return;
    set({ probing: true });
    try {
      const nodes = await api.serversRefresh();
      set({ nodes, loaded: true });
    } catch (e) {
      useApp.getState().toast("error", `节点探测失败:${String(e)}`);
    } finally {
      set({ probing: false });
    }
  },

  applyProbe: (probes) => {
    const map = new Map(probes.map((p) => [p.id, p]));
    set((s) => ({
      nodes: s.nodes.map((n) => {
        const p = map.get(n.id);
        return p ? { ...n, alive: p.alive, latency_ms: p.latencyMs } : n;
      }),
    }));
  },
}));

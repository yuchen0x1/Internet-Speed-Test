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
  const nextTime = samples.length ? samples[samples.length - 1][0] + 0.2 : 0;
  return [...samples.slice(-(MAX_SAMPLES - 1)), [nextTime, mbps]];
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
  /** 与后端引擎对账:托盘启动测速或窗口重载后,前端可能错过事件而状态停在 idle */
  syncRunning: () => Promise<void>;
}

let elapsedTimer: number | null = null;
let binding: Promise<void> | null = null;
let eventRevision = 0;
let syncSequence = 0;

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
    if (binding) return binding;
    binding = (async () => {
      const offEngine = await onEngineEvent((ev) => {
        eventRevision++;
        switch (ev.type) {
          case "phase": {
            const running = ev.phase !== "finished" && ev.phase !== "idle";
            const newRun = ev.phase === "preparing" && !get().running;
            set({
              phase: ev.phase as Phase,
              message: ev.message,
              running,
              progress: 0,
              error: null,
              ...(newRun
                ? { elapsedMs: 0, node: null, downSamples: [], upSamples: [], latencySamples: [], loadedDown: [], loadedUp: [], downloadedMb: 0, uploadedMb: 0, liveDown: 0, liveUp: 0, result: null }
                : {}),
            });
            if (!running) stopTimer();
            if (running && elapsedTimer == null) {
              const t0 = performance.now() - get().elapsedMs;
              elapsedTimer = window.setInterval(
                () => set((s) => ({ elapsedMs: s.running ? performance.now() - t0 : s.elapsedMs })),
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
              progress: ev.result.status === "completed" ? 100 : 0,
              message: ev.result.status === "completed" ? "测速完成" : ev.result.status === "cancelled" ? "已取消" : "测速失败",
              liveDown: 0,
              liveUp: 0,
              elapsedMs: ev.result.duration_ms,
              error: ev.result.error,
            });
            if (ev.result.history_save_error) useApp.getState().toast("warn", ev.result.history_save_error);
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
      try {
        await onServersProbed((probes: ProbeResult[]) => {
          useServers.getState().applyProbe(probes);
        });
        set({ bound: true });
      } catch (e) {
        offEngine();
        throw e;
      }
    })().finally(() => { binding = null; });
    return binding;
  },

  reset: () => {
    eventRevision++;
    stopTimer();
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
    });
  },

  syncRunning: async () => {
    const revision = eventRevision;
    const sequence = ++syncSequence;
    // 托盘菜单可以直接启动测速,窗口重载也会丢失事件订阅期间的状态:
    // 这两种情况下前端 store 会一直显示 idle,而引擎实际在跑。
    // 以后端的真实运行状态为准做一次对账,不猜测阶段细节(缺失的样本无法补造)。
    let running: boolean;
    let backendPhase = "";
    try {
      // 阶段与运行状态一起取:阶段能说明"正在测哪一步",不必猜测
      const [r, p] = await Promise.all([api.speedtestRunning(), api.speedtestPhase()]);
      running = r;
      backendPhase = p;
    } catch {
      return; // 查询失败时保持现状,不误改用户可见状态
    }
    if (revision !== eventRevision || sequence !== syncSequence) return;
    const cur = get();
    if (running && !cur.running) {
      const phase = (["preparing", "latency", "download", "upload", "quality"] as Phase[])
        .includes(backendPhase as Phase)
        ? (backendPhase as Phase)
        : "preparing";
      get().reset();
      set({
        running: true,
        phase,
        message: `测速正在进行中(后端阶段:${phase},由托盘或后台启动)`,
        error: null,
      });
      if (elapsedTimer == null) {
        const t0 = performance.now();
        elapsedTimer = window.setInterval(
          () => set((s) => ({ elapsedMs: s.running ? performance.now() - t0 : s.elapsedMs })),
          200,
        );
      }
    } else if (!running && cur.running) {
      // 引擎已结束但没收到 finished 事件:解除 running,保留已有结果,不伪造数值
      stopTimer();
      set({ running: false, phase: "finished", message: "测速已结束" });
    }
  },
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

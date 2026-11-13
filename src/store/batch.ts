// A run ID isolates late events; snapshots recover progress after a WebView reload.
import { create } from "zustand";
import { api, errMsg, onBatchUpdate } from "../api";
import { useApp } from "./app";
import type { BatchSnapshot, BatchTargetResult, DnsDistributionReport } from "../types";

const samples = new Map<string, (number | null)[]>();
export function getBatchSamples() { return samples; }

export const DEFAULT_TARGETS = [
  "223.5.5.5",
  "119.29.29.29",
  "www.baidu.com",
  "www.qq.com",
  "speed.cloudflare.com",
  "github.com",
].join("\n");

interface BatchStore {
  targetsText: string;
  count: number;
  running: boolean;
  busy: boolean;
  total: number;
  doneCount: number;
  results: BatchTargetResult[];
  activeTargets: string[];
  samplesVer: number;
  snapshot: BatchSnapshot | null;
  error: string | null;
  dnsDomain: string;
  dnsDist: DnsDistributionReport | null;
  dnsRun: boolean;
  bound: boolean;
  bind: () => Promise<void>;
  sync: () => Promise<void>;
  start: () => Promise<void>;
  stop: () => Promise<void>;
  applySnapshot: (snapshot: BatchSnapshot) => void;
  setTargetsText: (v: string) => void;
  setCount: (v: number) => void;
  setDnsDomain: (v: string) => void;
  setDnsDist: (v: DnsDistributionReport | null) => void;
  setDnsRun: (v: boolean) => void;
}

let binding: Promise<void> | null = null;
export const useBatch = create<BatchStore>((set, get) => ({
  targetsText: DEFAULT_TARGETS, count: 20, running: false, busy: false,
  total: 0, doneCount: 0, results: [], activeTargets: [], samplesVer: 0,
  snapshot: null, error: null,
  dnsDomain: "www.baidu.com", dnsDist: null, dnsRun: false, bound: false,

  applySnapshot: (incoming) => {
    const current = get().snapshot;
    if (current && incoming.run_id < current.run_id) return;
    const sameRun = current?.run_id === incoming.run_id;
    const results = new Map((sameRun ? current.results : []).map((r) => [r.target, r]));
    for (const r of incoming.results) {
      if (r.sent >= (results.get(r.target)?.sent ?? 0)) results.set(r.target, r);
    }
    // A delayed IPC snapshot may contain earlier metadata and samples.
    const meta = sameRun && current.revision > incoming.revision ? current : incoming;
    const snapshot = { ...meta, results: [...results.values()] };
    samples.clear();
    for (const r of snapshot.results) samples.set(r.target, r.samples);
    set((s) => ({
      snapshot, results: snapshot.results, activeTargets: snapshot.targets,
      running: snapshot.running, total: snapshot.targets.length * snapshot.count,
      doneCount: snapshot.results.reduce((sum, r) => sum + r.sent, 0),
      samplesVer: s.samplesVer + 1,
      ...(!sameRun ? { targetsText: snapshot.targets.join("\n"), count: snapshot.count } : {}),
    }));
  },

  sync: async () => {
    const snapshot = await api.batchtestSnapshot();
    if (snapshot) get().applySnapshot(snapshot);
    set({ error: null });
  },

  bind: async () => {
    if (get().bound) return;
    if (binding) return binding;
    binding = (async () => {
      const off = await onBatchUpdate((snapshot) => {
        const before = get().snapshot;
        get().applySnapshot(snapshot);
        if (before?.run_id === snapshot.run_id && before.running && !get().running) {
          useApp.getState().toast(snapshot.cancelled ? "warn" : "success",
            snapshot.cancelled ? "批量拨测已停止，已保留实际样本" : "批量拨测完成");
        }
      });
      try {
        await get().sync();
        set({ bound: true, error: null });
      } catch (e) { off(); throw e; }
    })().catch((e) => {
      set({ error: `拨测状态连接失败：${errMsg(e)}` });
    }).finally(() => { binding = null; });
    return binding;
  },

  start: async () => {
    if (get().busy || get().running) return;
    set({ busy: true, error: null });
    try {
      await get().bind();
      if (!get().bound) throw new Error("未连接拨测状态，请重试");
      const { targetsText, count } = get();
      await api.batchtestStart(targetsText.split("\n"), count);
      // Listeners are ready before start; do not clear samples that beat the IPC response.
      await get().sync();
    } catch (e) {
      set({ error: errMsg(e) });
      useApp.getState().toast("error", errMsg(e));
    } finally { set({ busy: false }); }
  },

  stop: async () => {
    if (get().busy) return;
    set({ busy: true, error: null });
    try { await api.batchtestStop(); await get().sync(); }
    catch (e) { set({ error: errMsg(e) }); useApp.getState().toast("error", errMsg(e)); }
    finally { set({ busy: false }); }
  },

  setTargetsText: (v) => set({ targetsText: v }),
  setCount: (v) => set({ count: v }),
  setDnsDomain: (v) => set({ dnsDomain: v }),
  setDnsDist: (v) => set({ dnsDist: v }),
  setDnsRun: (v) => set({ dnsRun: v }),
}));

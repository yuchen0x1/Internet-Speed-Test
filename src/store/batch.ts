// 批量拨测跨页状态:事件监听在 store 层绑定一次(切页/卸载不丢样本),
// 实时样本存模块级 Map,由 samplesVer 计数触发表格重绘。
import { create } from "zustand";
import { onBatchAllDone, onBatchTargetDone, onBatchTick } from "../api";
import { useApp } from "./app";
import type { BatchTargetResult, DnsDistributionReport } from "../types";

/** 实时样本序列(目标 → RTT 列表);不参与渲染状态,配合 samplesVer 使用 */
const samples = new Map<string, (number | null)[]>();

export function getBatchSamples(): Map<string, (number | null)[]> {
  return samples;
}

/** 默认拨测目标(每行一个) */
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
  total: number;
  doneCount: number;
  results: BatchTargetResult[];
  samplesVer: number;
  dnsDomain: string;
  dnsDist: DnsDistributionReport | null;
  dnsRun: boolean;
  bound: boolean;

  bind: () => Promise<void>;
  setTargetsText: (v: string) => void;
  setCount: (v: number) => void;
  setRunning: (v: boolean) => void;
  prepareRun: (total: number) => void;
  setDnsDomain: (v: string) => void;
  setDnsDist: (v: DnsDistributionReport | null) => void;
  setDnsRun: (v: boolean) => void;
}

export const useBatch = create<BatchStore>((set, get) => ({
  targetsText: DEFAULT_TARGETS,
  count: 20,
  running: false,
  total: 0,
  doneCount: 0,
  results: [],
  samplesVer: 0,
  dnsDomain: "www.baidu.com",
  dnsDist: null,
  dnsRun: false,
  bound: false,

  bind: async () => {
    if (get().bound) return;
    set({ bound: true });
    await onBatchTick((t) => {
      const arr = samples.get(t.target) ?? [];
      arr.push(t.rtt_ms);
      samples.set(t.target, arr);
      set((s) => ({ samplesVer: s.samplesVer + 1 }));
    });
    await onBatchTargetDone((r) =>
      set((s) => ({
        results: [...s.results.filter((x) => x.target !== r.target), r],
        doneCount: s.doneCount + 1,
      })),
    );
    await onBatchAllDone(() => {
      set({ running: false });
      useApp.getState().toast("success", "批量拨测完成");
    });
  },

  setTargetsText: (v) => set({ targetsText: v }),
  setCount: (v) => set({ count: v }),
  setRunning: (v) => set({ running: v }),
  prepareRun: (total) => {
    samples.clear();
    set({ results: [], doneCount: 0, total, samplesVer: 0 });
  },
  setDnsDomain: (v) => set({ dnsDomain: v }),
  setDnsDist: (v) => set({ dnsDist: v }),
  setDnsRun: (v) => set({ dnsRun: v }),
}));


// 跨页共享的 UI 状态:历史筛选/选择等,切页重挂载后保留
import { create } from "zustand";

interface HistoryUi {
  from: string;
  to: string;
  desc: boolean;
  selected: number[];
  setFrom: (v: string) => void;
  setTo: (v: string) => void;
  toggleDesc: () => void;
  setRange: (from: string, to: string) => void;
  setSelected: (v: number[]) => void;
  toggleSel: (id: number) => void;
}

export const useHistoryUi = create<HistoryUi>((set) => ({
  from: "",
  to: "",
  desc: true,
  selected: [],

  setFrom: (v) => set({ from: v }),
  setTo: (v) => set({ to: v }),
  toggleDesc: () => set((s) => ({ desc: !s.desc })),
  setRange: (from, to) => set({ from, to }),
  setSelected: (v) => set({ selected: v }),
  toggleSel: (id) =>
    set((s) => ({
      selected: s.selected.includes(id)
        ? s.selected.filter((x) => x !== id)
        : s.selected.length >= 2
          ? [s.selected[1], id]
          : [...s.selected, id],
    })),
}));

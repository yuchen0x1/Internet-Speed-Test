import { beforeEach, describe, expect, it, vi } from "vitest";
import { api, onBatchUpdate } from "../api";
import type { BatchSnapshot } from "../types";
const { toast } = vi.hoisted(() => ({ toast: vi.fn() }));
vi.mock("../api", () => ({
  api: { batchtestSnapshot: vi.fn(), batchtestStart: vi.fn(), batchtestStop: vi.fn() },
  onBatchUpdate: vi.fn(), errMsg: (e: unknown) => String(e),
}));
vi.mock("./app", () => ({ useApp: { getState: () => ({ toast }) } }));
let batch: typeof import("./batch");
let emit: (s: BatchSnapshot) => void;
const off = vi.fn();
const snapshot = (patch: Partial<BatchSnapshot> = {}): BatchSnapshot => ({
  run_id: 1, started_at: "2026-11-13T12:00:00+08:00", revision: 0, running: true,
  cancelled: false, count: 10, targets: ["localhost"], results: [], ...patch,
});
const result = (samples: (number | null)[]) => ({
  target: "localhost", sent: samples.length, recv: samples.filter((v) => v != null).length,
  loss_percent: 50, min_ms: 1, max_ms: 1, avg_ms: 1, latest_ms: samples.at(-1) ?? null, samples,
});
beforeEach(async () => {
  vi.resetModules(); vi.resetAllMocks();
  batch = await import("./batch");
  vi.mocked(onBatchUpdate).mockImplementation(async (handler) => { emit = handler; return off; });
  vi.mocked(api.batchtestSnapshot).mockResolvedValue(null);
  vi.mocked(api.batchtestStart).mockResolvedValue(1);
  vi.mocked(api.batchtestStop).mockResolvedValue(undefined);
});
describe("batch run lifecycle", () => {
  it("retains early samples and completion arriving before the start response", async () => {
    await batch.useBatch.getState().bind();
    vi.mocked(api.batchtestStart).mockImplementationOnce(async () => {
      emit(snapshot({ revision: 2, running: false, results: [result([1, null])] }));
      return 1;
    });
    vi.mocked(api.batchtestSnapshot).mockResolvedValueOnce(snapshot());
    await batch.useBatch.getState().start();
    expect(batch.useBatch.getState()).toMatchObject({ running: false, doneCount: 2, total: 10 });
    expect(batch.getBatchSamples().get("localhost")).toEqual([1, null]);
  });
  it("ignores previous runs and never regresses from a delayed snapshot", () => {
    const apply = batch.useBatch.getState().applySnapshot;
    apply(snapshot({ run_id: 2, revision: 5, running: false, cancelled: true, results: [result([1, null])] }));
    apply(snapshot({ run_id: 1, revision: 100, results: [result([999])] }));
    apply(snapshot({ run_id: 2, revision: 0 }));
    expect(batch.useBatch.getState()).toMatchObject({ running: false, doneCount: 2, snapshot: { run_id: 2, cancelled: true } });
  });
  it("blocks double clicks while awaiting IPC", async () => {
    let release!: () => void;
    vi.mocked(api.batchtestStart).mockImplementationOnce(() => new Promise((resolve) => { release = () => resolve(1); }));
    await batch.useBatch.getState().bind();
    const first = batch.useBatch.getState().start();
    await vi.waitFor(() => expect(api.batchtestStart).toHaveBeenCalledTimes(1));
    await batch.useBatch.getState().start();
    expect(api.batchtestStart).toHaveBeenCalledTimes(1);
    release(); await first;
  });
  it("unsubscribes after sync failure and can retry binding", async () => {
    vi.mocked(api.batchtestSnapshot).mockRejectedValueOnce(new Error("disconnected"));
    await batch.useBatch.getState().bind();
    expect(off).toHaveBeenCalledOnce();
    expect(batch.useBatch.getState().bound).toBe(false);
    await batch.useBatch.getState().bind();
    expect(batch.useBatch.getState()).toMatchObject({ bound: true, error: null });
  });
  it("does not start probes when the listener is unavailable", async () => {
    vi.mocked(onBatchUpdate).mockRejectedValueOnce(new Error("not ready"));
    await batch.useBatch.getState().start();
    expect(api.batchtestStart).not.toHaveBeenCalled();
    expect(batch.useBatch.getState().busy).toBe(false);
  });
  it("shares a handled failure across concurrent bindings", async () => {
    vi.mocked(onBatchUpdate).mockRejectedValueOnce(new Error("not ready"));
    await expect(Promise.all([batch.useBatch.getState().bind(), batch.useBatch.getState().bind()])).resolves.toEqual([undefined, undefined]);
    expect(onBatchUpdate).toHaveBeenCalledTimes(1);
    expect(batch.useBatch.getState().bound).toBe(false);
    await batch.useBatch.getState().bind();
    expect(batch.useBatch.getState().bound).toBe(true);
  });
  it("retains a running task when stop fails and distinguishes cancellation", async () => {
    await batch.useBatch.getState().bind();
    emit(snapshot({ revision: 1, results: [result([1])] }));
    vi.mocked(api.batchtestStop).mockRejectedValueOnce(new Error("IPC failed"));
    await batch.useBatch.getState().stop();
    expect(batch.useBatch.getState()).toMatchObject({ running: true, doneCount: 1 });
    emit(snapshot({ revision: 2, running: false, cancelled: true, results: [result([1])] }));
    expect(toast).toHaveBeenLastCalledWith("warn", expect.stringContaining("已停止"));
  });
  it("restores backend state without replaying completion notifications", async () => {
    vi.mocked(api.batchtestSnapshot).mockResolvedValueOnce(snapshot({ running: false, cancelled: true, results: [result([1, null])] }));
    await batch.useBatch.getState().bind();
    expect(batch.useBatch.getState()).toMatchObject({ running: false, doneCount: 2, targetsText: "localhost", count: 10 });
    expect(toast).not.toHaveBeenCalled();
  });
});

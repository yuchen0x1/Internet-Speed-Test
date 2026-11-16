import { describe, expect, it, vi } from "vitest";
import { runDiagnosticBatch } from "./diagnostics";

describe("runDiagnosticBatch", () => {
  it("limits in-flight checks and keeps running after a failure", async () => {
    let inFlight = 0;
    let peak = 0;
    const progress: number[] = [];
    const task = (id: string, delay: number, fail = false) => ({
      id,
      label: id,
      run: async () => {
        inFlight += 1;
        peak = Math.max(peak, inFlight);
        await new Promise((resolve) => setTimeout(resolve, delay));
        inFlight -= 1;
        if (fail) throw new Error(`${id} failed`);
      },
    });

    const result = await runDiagnosticBatch(
      [task("a", 5), task("b", 1, true), task("c", 1), task("d", 1)],
      2,
      (p) => progress.push(p.completed),
    );

    expect(peak).toBeLessThanOrEqual(2);
    expect(result.map((r) => r.id)).toEqual(["a", "b", "c", "d"]);
    expect(result.find((r) => r.id === "b")).toMatchObject({ ok: false, error: "b failed" });
    expect(result.filter((r) => r.ok).map((r) => r.id)).toEqual(["a", "c", "d"]);
    expect(progress).toEqual([1, 2, 3, 4]);
  });

  it("normalizes invalid concurrency and reports empty batches", async () => {
    const callback = vi.fn();
    await expect(runDiagnosticBatch([], 0, callback)).resolves.toEqual([]);
    expect(callback).toHaveBeenCalledWith({ completed: 0, total: 0, results: [] });
    await expect(runDiagnosticBatch([{ id: "only", label: "Only", run: async () => {} }], 0)).resolves.toEqual([
      { id: "only", label: "Only", ok: true },
    ]);
  });
});

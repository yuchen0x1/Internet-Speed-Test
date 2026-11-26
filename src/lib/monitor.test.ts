import { describe, expect, it } from "vitest";
import type { MonitorBucket } from "../types";
import { summarizeMonitorBuckets } from "./monitor";

const bucket = (sample_count: number, rx_avg_bps: number, tx_avg_bps: number): MonitorBucket => ({
  start_ts_ms: 0, end_ts_ms: 60000, sample_count, rx_avg_bps, tx_avg_bps,
  rx_p50_bps: rx_avg_bps, rx_p95_bps: rx_avg_bps, rx_peak_bps: rx_avg_bps,
  tx_p50_bps: tx_avg_bps, tx_p95_bps: tx_avg_bps, tx_peak_bps: tx_avg_bps,
});

describe("monitor aggregate summary", () => {
  it("weights partial time buckets by their actual sample counts", () => {
    expect(summarizeMonitorBuckets([bucket(60, 100, 30), bucket(1, 710, 91)]))
      .toEqual({ rxAvg: 110, txAvg: 31, rxP95: 710, txP95: 91, samples: 61 });
  });

  it("does not display a fabricated average for empty buckets", () => {
    expect(summarizeMonitorBuckets([])).toBeNull();
    expect(summarizeMonitorBuckets([bucket(0, 0, 0)])).toBeNull();
  });
});

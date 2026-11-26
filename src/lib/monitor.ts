import type { MonitorBucket } from "../types";

export function summarizeMonitorBuckets(buckets: MonitorBucket[]) {
  const samples = buckets.reduce((sum, bucket) => sum + bucket.sample_count, 0);
  if (!samples) return null;
  return {
    rxAvg: buckets.reduce((sum, bucket) => sum + bucket.rx_avg_bps * bucket.sample_count, 0) / samples,
    txAvg: buckets.reduce((sum, bucket) => sum + bucket.tx_avg_bps * bucket.sample_count, 0) / samples,
    rxP95: Math.max(...buckets.map((bucket) => bucket.rx_p95_bps)),
    txP95: Math.max(...buckets.map((bucket) => bucket.tx_p95_bps)),
    samples,
  };
}

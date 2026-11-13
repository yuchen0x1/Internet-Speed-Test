import { describe, expect, it } from "vitest";
import { radarScore } from "./RadarChart";

describe("radar unknown values", () => {
  it("keeps missing loss distinct from a measured zero percent", () => {
    const metric = { label: "丢包", max: 5, lowerBetter: true, display: "未知" };
    expect(radarScore({ ...metric, value: null })).toBeNull();
    expect(radarScore({ ...metric, value: 0 })).toBe(1);
    expect(radarScore({ ...metric, value: 5 })).toBe(0);
    expect(radarScore({ ...metric, value: Number.NaN })).toBeNull();
  });
});

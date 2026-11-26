import { describe, expect, it } from "vitest";
import { isUpdateCheckDue } from "./updater";

describe("automatic update check interval", () => {
  const now = Date.UTC(2026, 10, 27);
  const interval = 24 * 60 * 60 * 1000;

  it("respects a recent successful check and checks again when its interval expires", () => {
    expect(isUpdateCheckDue(now - 1000, interval, now)).toBe(false);
    expect(isUpdateCheckDue(now - interval, interval, now)).toBe(true);
  });

  it("recovers from a future stored timestamp after the system clock moves backwards", () => {
    expect(isUpdateCheckDue(now + interval * 30, interval, now)).toBe(true);
  });

  it("checks when the timestamp is missing or invalid", () => {
    for (const last of [0, -1, Number.NaN, Number.POSITIVE_INFINITY]) {
      expect(isUpdateCheckDue(last, interval, now)).toBe(true);
    }
  });
});

import { describe, expect, it } from "vitest";
import { fmtBytes, fmtBytesPerSec, fmtSpeedMbps, isCancelError, latencyPoints } from "./format";

describe("fmtBytesPerSec", () => {
  it("0 / 负数 / 非有限值统一显示 0 B/s", () => {
    expect(fmtBytesPerSec(0)).toBe("0 B/s");
    expect(fmtBytesPerSec(-1234)).toBe("0 B/s");
    expect(fmtBytesPerSec(NaN)).toBe("0 B/s");
    expect(fmtBytesPerSec(Infinity)).toBe("0 B/s");
  });

  it("B/s 档:≥100 取整、<100 保留一位小数", () => {
    expect(fmtBytesPerSec(512)).toBe("512 B/s");
    expect(fmtBytesPerSec(99.4)).toBe("99.4 B/s");
  });

  it("1024 进位到 KB/s", () => {
    expect(fmtBytesPerSec(1024)).toBe("1.0 KB/s");
    expect(fmtBytesPerSec(1536)).toBe("1.5 KB/s");
  });

  it("MB/s 档:数值 ≥100 时取整显示", () => {
    expect(fmtBytesPerSec(1024 * 1024)).toBe("1.0 MB/s");
    expect(fmtBytesPerSec(100 * 1024 * 1024)).toBe("100 MB/s");
  });

  it("GB/s 大值也能进位", () => {
    expect(fmtBytesPerSec(5 * 1024 ** 3)).toBe("5.0 GB/s");
    expect(fmtBytesPerSec(1024 ** 4)).toBe("1024 GB/s");
  });
});

describe("fmtBytes", () => {
  it("0 / 负数显示 0 B", () => {
    expect(fmtBytes(0)).toBe("0 B");
    expect(fmtBytes(-1)).toBe("0 B");
  });

  it("按 1024 进位并保留两位小数(<100)", () => {
    expect(fmtBytes(1024)).toBe("1.00 KB");
    expect(fmtBytes(1024 * 1024 * 5.5)).toBe("5.50 MB");
  });
});

describe("fmtSpeedMbps", () => {
  it("负数/非有限值显示 --", () => {
    expect(fmtSpeedMbps(-5).value).toBe("--");
    expect(fmtSpeedMbps(NaN).value).toBe("--");
  });

  it("按量程分档:≥1000 转 Gbps、≥100 一位小数、其余两位", () => {
    expect(fmtSpeedMbps(1500)).toEqual({ value: "1.50", unit: "Gbps" });
    expect(fmtSpeedMbps(123.4)).toEqual({ value: "123.4", unit: "Mbps" });
    expect(fmtSpeedMbps(42.5)).toEqual({ value: "42.50", unit: "Mbps" });
  });
});

describe("isCancelError", () => {
  it("识别用户取消类错误", () => {
    expect(isCancelError(new Error("用户取消了保存"))).toBe(true);
    expect(isCancelError("Error: 操作已取消")).toBe(true);
  });

  it("普通错误不算取消", () => {
    expect(isCancelError(new Error("磁盘已满"))).toBe(false);
    expect(isCancelError(undefined)).toBe(false);
  });
});

describe("latencyPoints", () => {
  it("x 轴口径 = 样本序号 × 0.35s,与原内联实现数值完全一致", () => {
    expect(latencyPoints([[1000, 12.5], [1001, 13], [1002, 14.25]])).toEqual([
      [0, 12.5],
      [0.35, 13],
      [0.7, 14.25],
    ]);
  });

  it("空样本返回空数组", () => {
    expect(latencyPoints([])).toEqual([]);
  });
});

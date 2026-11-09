import { describe, expect, it } from "vitest";
import { stabilityText, stabilityTone } from "./quality";

describe("stabilityText", () => {
  it("0 分(三项全不达标)也有文案,不再是空串", () => {
    expect(stabilityText(20, 3, 10)).toBe("较差");
  });

  it("1 分(仅抖动达标)→ 较弱", () => {
    expect(stabilityText(1, 3, 10)).toBe("较弱");
  });

  it("2 分(抖动+负载比达标)→ 中等", () => {
    expect(stabilityText(1, 1, 10)).toBe("中等");
  });

  it("3 分(满分)→ 良好", () => {
    expect(stabilityText(1, 1, 0)).toBe("良好");
  });

  it("ratio/loss 为 null 时不计分", () => {
    expect(stabilityText(20, null, null)).toBe("较差");
    expect(stabilityText(1, null, null)).toBe("较弱");
  });
});

describe("stabilityTone", () => {
  it("良好 → good、中等 → warn", () => {
    expect(stabilityTone(1, 1, 0)).toBe("good");
    expect(stabilityTone(1, 1, 10)).toBe("warn");
  });

  it("较弱/较差 → bad", () => {
    expect(stabilityTone(1, 3, 10)).toBe("bad");
    expect(stabilityTone(20, 3, 10)).toBe("bad");
  });
});

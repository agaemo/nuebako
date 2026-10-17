import { describe, expect, it } from "vitest";
import { windowHeight } from "./windowHeight";

describe("windowHeight", () => {
  it("10行のとき約330px 以内に収まる", () => {
    const h = windowHeight(10, false);
    expect(h).toBeLessThanOrEqual(330);
    expect(h).toBeGreaterThan(300);
  });
  it("行が減ると 24px ずつ低くなる", () => {
    expect(windowHeight(10, false) - windowHeight(9, false)).toBe(24);
  });
  it("10行を超える件数でも10行分で止まる", () => {
    expect(windowHeight(50, false)).toBe(windowHeight(10, false));
  });
  it("プレビューを開くと 96px 高くなる", () => {
    expect(windowHeight(5, true) - windowHeight(5, false)).toBe(96);
  });
  it("0件のときは案内文の分の高さを持つ", () => {
    expect(windowHeight(0, false)).toBeGreaterThan(windowHeight(0, false) - 1);
    expect(windowHeight(0, false)).toBeGreaterThan(80);
  });
});

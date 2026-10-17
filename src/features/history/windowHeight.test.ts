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
  it("画像のプレビューは 240px の高さになる(画像を見やすくする)", () => {
    expect(windowHeight(5, true, "image") - windowHeight(5, false)).toBe(240);
  });
  it("テキストなど画像以外のプレビューは 96px のまま", () => {
    expect(windowHeight(5, true, "text") - windowHeight(5, false)).toBe(96);
  });
  it("プレビューを閉じているときは、画像でも高さは変わらない", () => {
    expect(windowHeight(5, false, "image")).toBe(windowHeight(5, false));
  });
  it("0件のときは案内文の分の高さを持つ", () => {
    expect(windowHeight(0, false)).toBeGreaterThan(80);
  });
  it("10行を超える件数でも、行の高さは10行分で頭打ちになる(残りはスクロール)", () => {
    expect(windowHeight(50, false)).toBe(windowHeight(10, false));
  });
  it("状態の帯など、行以外の高さを足せる", () => {
    expect(windowHeight(3, false, undefined, 40) - windowHeight(3, false)).toBe(40);
  });
});

import { describe, expect, it } from "vitest";
import { digitPasteIndex } from "./keys";

// 共有ファイル。各トラックが自分のテストを追記する(相手のテストは変更しない)。
describe("keys", () => {
  it.todo("track-settings: toAccelerator のテストを追記する");
});

describe("digitPasteIndex(track-ui-history)", () => {
  const ev = (
    code: string,
    mods: Partial<Record<"metaKey" | "altKey" | "ctrlKey" | "shiftKey", boolean>> = {},
  ) => ({
    code,
    metaKey: false,
    altKey: false,
    ctrlKey: false,
    shiftKey: false,
    ...mods,
  });

  it("macOS で Cmd+Digit1〜9 のとき 0〜8 を返す", () => {
    for (let n = 1; n <= 9; n++)
      expect(digitPasteIndex(ev(`Digit${n}`, { metaKey: true }), "macos")).toBe(n - 1);
  });

  it("macOS で Cmd+Digit0 のとき 9(10行目)を返す", () => {
    expect(digitPasteIndex(ev("Digit0", { metaKey: true }), "macos")).toBe(9);
  });

  it("Windows で Alt+Digit1 と Alt+Digit0 のとき 0 と 9 を返す", () => {
    expect(digitPasteIndex(ev("Digit1", { altKey: true }), "windows")).toBe(0);
    expect(digitPasteIndex(ev("Digit0", { altKey: true }), "windows")).toBe(9);
  });

  it("修飾キーなしの数字のとき null(検索欄への入力)を返す", () => {
    expect(digitPasteIndex(ev("Digit3"), "macos")).toBeNull();
    expect(digitPasteIndex(ev("Digit3"), "windows")).toBeNull();
  });

  it("OS と違う修飾キーのとき null を返す", () => {
    expect(digitPasteIndex(ev("Digit3", { altKey: true }), "macos")).toBeNull();
    expect(digitPasteIndex(ev("Digit3", { metaKey: true }), "windows")).toBeNull();
  });

  it("余計な修飾キーが付いているとき null を返す", () => {
    expect(digitPasteIndex(ev("Digit3", { metaKey: true, shiftKey: true }), "macos")).toBeNull();
    expect(digitPasteIndex(ev("Digit3", { altKey: true, ctrlKey: true }), "windows")).toBeNull();
  });

  it("数字以外のキーと、テンキーのとき null を返す", () => {
    expect(digitPasteIndex(ev("KeyA", { metaKey: true }), "macos")).toBeNull();
    expect(digitPasteIndex(ev("Numpad1", { metaKey: true }), "macos")).toBeNull();
  });
});

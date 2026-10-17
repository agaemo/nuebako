import { describe, expect, it } from "vitest";
import { digitPasteIndex, toAccelerator } from "./keys";

// 共有ファイル。各トラックが自分のテストを追記する。
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

// ---- track-settings: toAccelerator ----
describe("toAccelerator", () => {
  const ev = (
    code: string,
    mods: Partial<Record<"metaKey" | "ctrlKey" | "altKey" | "shiftKey", boolean>> = {},
  ) => ({
    code,
    metaKey: false,
    ctrlKey: false,
    altKey: false,
    shiftKey: false,
    ...mods,
  });

  it("Ctrl+Shift+V のとき Control+Shift+V になる", () => {
    expect(toAccelerator(ev("KeyV", { ctrlKey: true, shiftKey: true }))).toBe("Control+Shift+V");
  });

  it("Cmd+Shift+V のとき Shift+Command+V になる", () => {
    expect(toAccelerator(ev("KeyV", { metaKey: true, shiftKey: true }))).toBe("Shift+Command+V");
  });

  it("修飾キーが複数のとき Control、Alt、Shift、Command の順に並ぶ", () => {
    expect(
      toAccelerator(ev("KeyK", { metaKey: true, shiftKey: true, altKey: true, ctrlKey: true })),
    ).toBe("Control+Alt+Shift+Command+K");
  });

  it("修飾キーが無いとき null", () => {
    expect(toAccelerator(ev("KeyV"))).toBeNull();
    expect(toAccelerator(ev("F1"))).toBeNull();
  });

  it("修飾キーだけのとき null", () => {
    expect(toAccelerator(ev("ShiftLeft", { shiftKey: true }))).toBeNull();
    expect(toAccelerator(ev("ControlLeft", { ctrlKey: true }))).toBeNull();
    expect(toAccelerator(ev("MetaLeft", { metaKey: true }))).toBeNull();
  });

  it("数字・ファンクション・矢印・記号キーを変換する", () => {
    expect(toAccelerator(ev("Digit1", { altKey: true }))).toBe("Alt+1");
    expect(toAccelerator(ev("F12", { ctrlKey: true }))).toBe("Control+F12");
    expect(toAccelerator(ev("ArrowUp", { ctrlKey: true }))).toBe("Control+Up");
    expect(toAccelerator(ev("Space", { altKey: true }))).toBe("Alt+Space");
    expect(toAccelerator(ev("Slash", { ctrlKey: true }))).toBe("Control+/");
  });

  it("対応しないキーのとき null", () => {
    expect(toAccelerator(ev("IntlYen", { ctrlKey: true }))).toBeNull();
    expect(toAccelerator(ev("Escape", { ctrlKey: true }))).toBeNull();
  });
});

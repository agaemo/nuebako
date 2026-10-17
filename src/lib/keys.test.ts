import { describe, expect, it } from "vitest";
import { toAccelerator } from "./keys";

// 共有ファイル。各トラックが自分のテストを追記する(相手のテストは変更しない)。
describe("keys", () => {
  it.todo("track-ui-history: 番号貼り付けの解釈のテストを追記する");
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

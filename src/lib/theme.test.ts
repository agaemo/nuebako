import { describe, expect, it } from "vitest";
import { applyTheme } from "./theme";

describe("applyTheme", () => {
  it("darkのとき、data-themeにdarkが入る", () => {
    const root = document.createElement("html");
    applyTheme("dark", root);
    expect(root.getAttribute("data-theme")).toBe("dark");
  });

  it("systemのとき、data-themeが外れてOSの設定に任せる", () => {
    const root = document.createElement("html");
    root.setAttribute("data-theme", "light");
    applyTheme("system", root);
    expect(root.hasAttribute("data-theme")).toBe(false);
  });
});

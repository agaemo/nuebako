import type { Theme } from "../types";

/** テーマを反映する。system のときは data-theme を外し、OS の設定(prefers-color-scheme)に任せる。 */
export function applyTheme(theme: Theme, root: HTMLElement = document.documentElement): void {
  if (theme === "system") {
    root.removeAttribute("data-theme");
  } else {
    root.setAttribute("data-theme", theme);
  }
}

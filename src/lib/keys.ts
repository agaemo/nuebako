// キー操作の解釈。設定画面のショートカット入力(toAccelerator)と、
// 履歴ウィンドウの番号貼り付け(digitPasteIndex)の、2つの画面が使う。

// ---- ショートカットの文字列化 ----

/** toAccelerator が読むキーボードイベントの項目。 */
export type AcceleratorEvent = Pick<
  KeyboardEvent,
  "code" | "metaKey" | "ctrlKey" | "altKey" | "shiftKey"
>;

/** `KeyboardEvent.code` から、ショートカット文字列のキー名への対応(英字・数字・ファンクションキーは別に処理する)。 */
const CODE_NAMES: Record<string, string> = {
  Space: "Space",
  Enter: "Enter",
  Tab: "Tab",
  Backspace: "Backspace",
  Delete: "Delete",
  Insert: "Insert",
  Home: "Home",
  End: "End",
  PageUp: "PageUp",
  PageDown: "PageDown",
  ArrowUp: "Up",
  ArrowDown: "Down",
  ArrowLeft: "Left",
  ArrowRight: "Right",
  Minus: "-",
  Equal: "=",
  BracketLeft: "[",
  BracketRight: "]",
  Backslash: "\\",
  Semicolon: ";",
  Quote: "'",
  Backquote: "`",
  Comma: ",",
  Period: ".",
  Slash: "/",
};

/**
 * キー入力を、Tauri のショートカット文字列(例: `Control+Shift+V`)にする。
 * 修飾キーが無い、修飾キーだけ、または対応しないキーのときは null。
 * 並び順は Control、Alt、Shift、Command(Windows キーも Command として扱う)。
 */
export function toAccelerator(e: AcceleratorEvent): string | null {
  const modifiers: string[] = [];
  if (e.ctrlKey) modifiers.push("Control");
  if (e.altKey) modifiers.push("Alt");
  if (e.shiftKey) modifiers.push("Shift");
  if (e.metaKey) modifiers.push("Command");
  if (modifiers.length === 0) return null;

  const key = keyName(e.code);
  return key === null ? null : [...modifiers, key].join("+");
}

function keyName(code: string): string | null {
  const letter = /^Key([A-Z])$/.exec(code);
  if (letter) return letter[1];
  const digit = /^Digit([0-9])$/.exec(code);
  if (digit) return digit[1];
  if (/^F([1-9]|1[0-9]|2[0-4])$/.test(code)) return code;
  return CODE_NAMES[code] ?? null;
}

// ---- 番号貼り付けの解釈 ----

/** 番号貼り付けの判定に使うキーイベントの最小限の形 */
export interface DigitPasteEvent {
  code: string;
  metaKey: boolean;
  altKey: boolean;
  ctrlKey: boolean;
  shiftKey: boolean;
}

/**
 * 番号貼り付けのキー(macOS は Cmd+数字、それ以外は Alt+数字)を、行の位置(0 始まり)に変換する。
 * Digit1〜Digit9 は 0〜8、Digit0 は 9(10行目)。対象外のキーは null。
 * 物理キー(event.code)で判定するので、キー配列や IME の影響を受けない。
 * 修飾キーなしの数字は null(検索欄への入力になる)。余計な修飾キーが付いたものも対象にしない。
 */
export function digitPasteIndex(e: DigitPasteEvent, os: string): number | null {
  const m = /^Digit([0-9])$/.exec(e.code);
  if (!m) return null;
  const isMac = os === "macos";
  const modifierOk = isMac
    ? e.metaKey && !e.altKey && !e.ctrlKey && !e.shiftKey
    : e.altKey && !e.metaKey && !e.ctrlKey && !e.shiftKey;
  if (!modifierOk) return null;
  const n = Number(m[1]);
  return n === 0 ? 9 : n - 1;
}

// キー操作の解釈。2つのトラックの共有ファイル。
// - track-settings: toAccelerator(KeyboardEvent)(ショートカットの文字列化)を追記する
// - track-ui-history: 番号貼り付けの解釈を追記する
// それぞれ自分の関数を追加するだけで、相手の関数は変更しない。
export {};

// ---- track-settings: toAccelerator ----

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

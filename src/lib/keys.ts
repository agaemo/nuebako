// キー操作の解釈。2つのトラックの共有ファイル。
// - track-settings: toAccelerator(KeyboardEvent)(ショートカットの文字列化)を追記する
// - track-ui-history: 番号貼り付けの解釈を追記する
// それぞれ自分の関数を追加するだけで、相手の関数は変更しない。
export {};

// ---- track-ui-history: 番号貼り付けの解釈 ----

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

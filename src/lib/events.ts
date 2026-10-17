// Rust からのイベントの購読。戻り値の関数を呼ぶと購読を解除する。
import { listen } from "@tauri-apps/api/event";
import type { Settings } from "../types";

type Unlisten = () => void;

async function subscribe<T>(name: string, handler: (payload: T) => void): Promise<Unlisten> {
  return listen<T>(name, (e) => handler(e.payload));
}

/** 記録・削除・上限処理の後。内容は含まれないので、一覧を取り直す。 */
export const onHistoryChanged = (handler: () => void) =>
  subscribe<null>("history-changed", () => handler());
/** 設定の更新後。 */
export const onSettingsChanged = (handler: (settings: Settings) => void) =>
  subscribe<Settings>("settings-changed", handler);
/** 履歴ウィンドウの表示時。検索欄にフォーカスし、選択を先頭に戻し、一覧を取り直す。 */
export const onHistoryWindowShown = (handler: () => void) =>
  subscribe<null>("history-window-shown", () => handler());

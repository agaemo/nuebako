// Rust のコマンド呼び出しを集約する。画面からは invoke を直接呼ばず、ここを使う。
// 注意: Tauri は、コマンドの引数名(snake_case)を camelCase に変換して受け取る(req_id → reqId)。
import { invoke } from "@tauri-apps/api/core";
import type {
  AppError,
  AppRef,
  AppStatus,
  ClipDetail,
  ClipPage,
  LimitPreview,
  PasteMode,
  PasteOutcome,
  Settings,
  SettingsPatch,
} from "../types";

export const getAppStatus = () => invoke<AppStatus>("get_app_status");
export const hideWindow = () => invoke<void>("hide_window");
export const openSettings = () => invoke<void>("open_settings");
export const openAccessibilitySettings = () => invoke<void>("open_accessibility_settings");

export const listClips = (query: string, reqId: number) =>
  invoke<ClipPage>("list_clips", { query, reqId });
export const getClipDetail = (id: number) => invoke<ClipDetail>("get_clip_detail", { id });
export const deleteClip = (id: number) => invoke<void>("delete_clip", { id });
export const clearHistory = () => invoke<number>("clear_history");
export const setPinned = (id: number, pinned: boolean) => invoke<void>("set_pinned", { id, pinned });

export const pasteClip = (id: number, mode: PasteMode) =>
  invoke<PasteOutcome>("paste_clip", { id, mode });

export const getSettings = () => invoke<Settings>("get_settings");
export const previewLimits = (maxItems: string, retentionDays: string) =>
  invoke<LimitPreview>("preview_limits", { maxItems, retentionDays });
export const updateSettings = (patch: SettingsPatch) => invoke<Settings>("update_settings", { patch });
export const setShortcut = (accelerator: string) => invoke<Settings>("set_shortcut", { accelerator });
export const listRunningApps = () => invoke<AppRef[]>("list_running_apps");

/** invoke の失敗(unknown)を AppError として読む。形が違うときは internal として扱う。 */
export function toAppError(e: unknown): AppError {
  if (e && typeof e === "object" && "code" in e && "message" in e) return e as AppError;
  return { code: "internal", message: String(e) };
}

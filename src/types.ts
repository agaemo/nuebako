// Rust の IPC の型(src-tauri/src/domain/dto.rs など)と対応させる。JSON のキーは snake_case。

export type ClipKind = "text" | "url" | "code" | "path" | "image";

export interface AppRef {
  /** macOS は bundle id、Windows は exe 名 */
  id: string;
  name: string;
}

export interface ClipSummary {
  id: number;
  kind: ClipKind;
  first_line: string;
  copied_at_ms: number;
  pinned: boolean;
  source_app_name: string | null;
  char_count: number;
  /** PNG を base64 にした文字列 */
  thumb_png: string | null;
}

export interface ClipPage {
  req_id: number;
  /** 履歴の全件数(検索条件に関わらない) */
  total: number;
  items: ClipSummary[];
}

export interface ClipDetail {
  id: number;
  kind: ClipKind;
  text: string | null;
  image_png: string | null;
  source_app_name: string | null;
  copied_at_ms: number;
  char_count: number;
}

export type PasteMode = "auto" | "copy_only";
export type PasteResult = "pasted" | "copied_only";
export type PasteReason = "no_permission" | "send_failed";

export interface PasteOutcome {
  result: PasteResult;
  reason: PasteReason | null;
}

export type StorageStatus = "ok" | "key_unavailable" | "key_mismatch" | "error";

export interface AppStatus {
  os: string;
  version: string;
  storage: StorageStatus;
  storage_message: string | null;
  paste_permission: boolean;
  permission_guide_dismissed: boolean;
}

export type Theme = "system" | "light" | "dark";

export interface Settings {
  shortcut: string;
  max_items: number;
  retention_days: number;
  theme: Theme;
  excluded_apps: AppRef[];
  record_unknown_source: boolean;
  permission_guide_dismissed: boolean;
}

/** 設定の部分更新。数値は文字列で渡し、Rust 側で検証する。 */
export interface SettingsPatch {
  max_items?: string;
  retention_days?: string;
  theme?: Theme;
  excluded_apps?: AppRef[];
  record_unknown_source?: boolean;
  permission_guide_dismissed?: boolean;
}

export interface FieldError {
  field: string;
  message: string;
}

export interface LimitPreview {
  valid: boolean;
  error: FieldError | null;
  will_delete: number;
}

export type AppErrorCode =
  | "key_unavailable"
  | "key_mismatch"
  | "storage"
  | "validation"
  | "shortcut_conflict"
  | "not_found"
  | "clipboard"
  | "permission_denied"
  | "internal";

/** Rust の AppError。invoke の失敗時に、この形で受け取る。 */
export interface AppError {
  code: AppErrorCode;
  message: string;
  field?: string;
}

//! IPC で画面と受け渡す型。JSON のキーは snake_case(`src/types.ts` と対応させる)。

use serde::{Deserialize, Serialize};

use super::clip::{AppRef, ClipKind};
use super::settings::Theme;

/// 一覧の1行分。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ClipSummary {
    pub id: i64,
    pub kind: ClipKind,
    /// 本文の最初の1行(最大200文字)
    pub first_line: String,
    pub copied_at_ms: i64,
    pub pinned: bool,
    pub source_app_name: Option<String>,
    pub char_count: u32,
    /// 画像のサムネイル(PNG を base64 にした文字列)
    pub thumb_png: Option<String>,
}

/// 検索結果。`req_id` は古い応答を捨てるための世代番号で、呼び出し元の値をそのまま返す。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ClipPage {
    pub req_id: u64,
    /// 履歴の全件数(検索条件に関わらない。ヘッダーの「全 N 件」)
    pub total: u32,
    pub items: Vec<ClipSummary>,
}

/// プレビュー用の全文または画像。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ClipDetail {
    pub id: i64,
    pub kind: ClipKind,
    pub text: Option<String>,
    /// PNG を base64 にした文字列
    pub image_png: Option<String>,
    pub source_app_name: Option<String>,
    pub copied_at_ms: i64,
    pub char_count: u32,
}

/// 貼り付けの結果。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PasteResult {
    Pasted,
    CopiedOnly,
}

/// 再コピーのみになった理由。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PasteReason {
    NoPermission,
    SendFailed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PasteOutcome {
    pub result: PasteResult,
    pub reason: Option<PasteReason>,
}

/// 貼り付けの方式。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PasteMode {
    Auto,
    CopyOnly,
}

/// 保存の状態(起動時の鍵と DB の判定結果)。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum StorageStatus {
    Ok,
    KeyUnavailable,
    KeyMismatch,
    Error,
}

/// 起動状態と権限。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct AppStatus {
    pub os: String,
    pub version: String,
    pub storage: StorageStatus,
    pub storage_message: Option<String>,
    pub paste_permission: bool,
    pub permission_guide_dismissed: bool,
}

/// 入力の検証エラー(どの欄か、何が悪いか)。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct FieldError {
    pub field: String,
    pub message: String,
}

/// 上限変更の事前確認。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct LimitPreview {
    pub valid: bool,
    pub error: Option<FieldError>,
    /// 適用すると削除される件数
    pub will_delete: u32,
}

/// 設定の部分更新。数値は文字列で受け取り、Rust 側で検証する。
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
pub struct SettingsPatch {
    pub max_items: Option<String>,
    pub retention_days: Option<String>,
    pub theme: Option<Theme>,
    pub excluded_apps: Option<Vec<AppRef>>,
    pub record_unknown_source: Option<bool>,
    pub permission_guide_dismissed: Option<bool>,
}

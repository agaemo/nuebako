//! 貼り付け。track-clipboard が実装する。
// 骨組み(P1-5)。本体は各トラックで実装し、P3-1 でこの属性を外す。
#![allow(unused_variables)]

use tauri::AppHandle;

use crate::domain::dto::{PasteMode, PasteOutcome};
use crate::error::AppError;

/// クリップボードへ設定、ウィンドウを閉じる、フォーカス復帰、(権限があれば)貼り付けを行う。
#[tauri::command]
pub fn paste_clip(app: AppHandle, id: i64, mode: PasteMode) -> Result<PasteOutcome, AppError> {
    Err(AppError::Internal("未実装(track-clipboard)".to_string()))
}

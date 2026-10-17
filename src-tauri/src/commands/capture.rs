//! 実行中のアプリの一覧(除外アプリの追加用)。track-clipboard が実装する。
// 骨組み(P1-5)。本体は各トラックで実装し、P3-1 でこの属性を外す。
#![allow(unused_variables)]

use crate::domain::clip::AppRef;
use crate::error::AppError;

#[tauri::command]
pub fn list_running_apps() -> Result<Vec<AppRef>, AppError> {
    Err(AppError::Internal("未実装(track-clipboard)".to_string()))
}

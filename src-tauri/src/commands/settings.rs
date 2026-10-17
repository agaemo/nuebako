//! 設定の取得・更新・ショートカット変更(track-settings が実装する)。
// 骨組み(P1-5)。本体は各トラックで実装し、P3-1 でこの属性を外す。
#![allow(unused_variables)]

use tauri::AppHandle;

use crate::domain::dto::{LimitPreview, SettingsPatch};
use crate::domain::settings::Settings;
use crate::error::AppError;

#[tauri::command]
pub fn get_settings(app: AppHandle) -> Result<Settings, AppError> {
    Err(AppError::Internal("未実装(track-settings)".to_string()))
}

/// 上限変更の検証と、削除される件数の事前計算(確認ダイアログ用)。
#[tauri::command]
pub fn preview_limits(app: AppHandle, max_items: String, retention_days: String) -> Result<LimitPreview, AppError> {
    Err(AppError::Internal("未実装(track-settings)".to_string()))
}

/// 設定を更新する。上限を変えたら、上限処理を行い、`settings-changed` と `history-changed` を発行する。
#[tauri::command]
pub fn update_settings(app: AppHandle, patch: SettingsPatch) -> Result<Settings, AppError> {
    Err(AppError::Internal("未実装(track-settings)".to_string()))
}

/// 新しいショートカットを登録し、成功したときだけ古いものを解除して保存する。失敗時は元を維持する。
#[tauri::command]
pub fn set_shortcut(app: AppHandle, accelerator: String) -> Result<Settings, AppError> {
    Err(AppError::Internal("未実装(track-settings)".to_string()))
}

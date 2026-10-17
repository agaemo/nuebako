//! 履歴の検索・詳細・削除・ピン留め(track-store が実装する)。
// 骨組み(P1-5)。本体は各トラックで実装し、P3-1 でこの属性を外す。
#![allow(unused_variables)]

use tauri::AppHandle;

use crate::domain::dto::{ClipDetail, ClipPage};
use crate::error::AppError;

#[tauri::command]
pub fn list_clips(app: AppHandle, query: String, req_id: u64) -> Result<ClipPage, AppError> {
    Err(AppError::Internal("未実装(track-store)".to_string()))
}

#[tauri::command]
pub fn get_clip_detail(app: AppHandle, id: i64) -> Result<ClipDetail, AppError> {
    Err(AppError::Internal("未実装(track-store)".to_string()))
}

#[tauri::command]
pub fn delete_clip(app: AppHandle, id: i64) -> Result<(), AppError> {
    Err(AppError::Internal("未実装(track-store)".to_string()))
}

/// 全件を削除し、削除した件数を返す。
#[tauri::command]
pub fn clear_history(app: AppHandle) -> Result<u32, AppError> {
    Err(AppError::Internal("未実装(track-store)".to_string()))
}

#[tauri::command]
pub fn set_pinned(app: AppHandle, id: i64, pinned: bool) -> Result<(), AppError> {
    Err(AppError::Internal("未実装(track-store)".to_string()))
}

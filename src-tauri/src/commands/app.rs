//! アプリの状態、ウィンドウ操作、OS の設定を開く。

use tauri::{AppHandle, Manager};

use crate::app_state::AppState;
use crate::domain::dto::{AppStatus, StorageStatus};
use crate::error::AppError;
use crate::{paste, window};

/// 起動状態、OS、権限を返す。状態が未登録のときは保存を `ok` として返す。
#[tauri::command]
pub fn get_app_status(app: AppHandle) -> Result<AppStatus, AppError> {
    let (storage, storage_message, guide_dismissed) = match app.try_state::<AppState>() {
        Some(state) => {
            let (status, message) = state
                .storage
                .lock()
                .map_err(|_| AppError::Internal("状態を読めません".to_string()))?
                .clone();
            (
                status,
                message,
                state.settings.get().permission_guide_dismissed,
            )
        }
        None => (StorageStatus::Ok, None, false),
    };
    Ok(AppStatus {
        os: std::env::consts::OS.to_string(),
        version: app.package_info().version.to_string(),
        storage,
        storage_message,
        paste_permission: paste::permission::current_permission(),
        permission_guide_dismissed: guide_dismissed,
    })
}

/// 履歴ウィンドウを閉じて、元のアプリにフォーカスを戻す。
#[tauri::command]
pub fn hide_window(app: AppHandle) {
    window::hide_history(&app);
}

/// 設定ウィンドウを開く。
#[tauri::command]
pub fn open_settings(app: AppHandle) {
    window::show_settings(&app);
}

/// macOS のアクセシビリティ設定を開く。他の OS では何もしない。
#[tauri::command]
pub fn open_accessibility_settings() -> Result<(), AppError> {
    #[cfg(target_os = "macos")]
    std::process::Command::new("open")
        .arg("x-apple.systempreferences:com.apple.preference.security?Privacy_Accessibility")
        .spawn()
        .map_err(|e| AppError::Internal(e.to_string()))?;
    Ok(())
}

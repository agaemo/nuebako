//! 実行中のアプリの一覧(除外アプリの追加用)。track-clipboard が実装する。
use tauri::{AppHandle, Manager};

use crate::app_state::AppState;
use crate::domain::clip::AppRef;
use crate::error::AppError;
use crate::platform::AppInspector;
use crate::platform::apps::{SELF_APP_ID, SystemAppInspector};

/// 実行中のアプリの一覧を返す(このアプリ自身は除く)。状態が未登録のときは `Internal` を返す。
#[tauri::command]
pub fn list_running_apps(app: AppHandle) -> Result<Vec<AppRef>, AppError> {
    if app.try_state::<AppState>().is_none() {
        return Err(AppError::Internal("アプリの状態が未登録です".to_string()));
    }
    let apps = SystemAppInspector.running_apps()?;
    Ok(apps
        .into_iter()
        .filter(|a| !a.id.eq_ignore_ascii_case(SELF_APP_ID))
        .collect())
}

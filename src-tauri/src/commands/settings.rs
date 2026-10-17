//! 設定の取得・更新・ショートカット変更。
use std::sync::Arc;

use tauri::{AppHandle, Emitter, Manager};

use crate::app_state::AppState;
use crate::domain::dto::{FieldError, LimitPreview, SettingsPatch};
use crate::domain::settings::Settings;
use crate::error::AppError;
use crate::history::HistoryService;
use crate::settings::shortcut::{TauriRegistrar, change_shortcut};
use crate::settings::store::SettingsStore;
use crate::settings::validate::validate_limits;

/// `AppState` から設定と履歴を取り出す。未登録のときは `Internal`(パニックしない)。
fn services(app: &AppHandle) -> Result<(Arc<SettingsStore>, Arc<HistoryService>), AppError> {
    let state = app
        .try_state::<AppState>()
        .ok_or_else(|| AppError::Internal("アプリの状態が初期化されていません".to_string()))?;
    Ok((state.settings.clone(), state.history.clone()))
}

/// 設定ファイルの読み書きや、履歴の件数の再計算を含むため、メインスレッドで動かさない(`async`)。
#[tauri::command(async)]
pub fn get_settings(app: AppHandle) -> Result<Settings, AppError> {
    Ok(services(&app)?.0.get())
}

/// 上限変更の検証と、削除される件数の事前計算(確認ダイアログ用)。
#[tauri::command(async)]
pub fn preview_limits(
    app: AppHandle,
    max_items: String,
    retention_days: String,
) -> Result<LimitPreview, AppError> {
    let (_, history) = services(&app)?;
    match validate_limits(&max_items, &retention_days) {
        Ok((max, days)) => Ok(LimitPreview {
            valid: true,
            error: None,
            will_delete: history.count_over_limits(max, days)?,
        }),
        Err(AppError::Validation { field, message }) => Ok(LimitPreview {
            valid: false,
            error: Some(FieldError { field, message }),
            will_delete: 0,
        }),
        Err(e) => Err(e),
    }
}

/// 設定を更新する。上限を変えたら、上限処理を行い、`settings-changed` と `history-changed` を発行する。
#[tauri::command(async)]
pub fn update_settings(app: AppHandle, patch: SettingsPatch) -> Result<Settings, AppError> {
    let (store, history) = services(&app)?;
    let before = store.get();
    let after = store.update(patch)?;
    let limits_changed =
        before.max_items != after.max_items || before.retention_days != after.retention_days;
    if limits_changed {
        history.apply_limits(after.max_items, after.retention_days)?;
    }
    let _ = app.emit("settings-changed", after.clone());
    if limits_changed {
        let _ = app.emit("history-changed", ());
    }
    Ok(after)
}

/// 新しいショートカットを登録し、成功したときだけ古いものを解除して保存する。失敗時は元を維持する。
// 以下の3つ(ショートカットの変更・一時解除・再開)は、登録をプラグインがメインスレッドで実行し、
// 呼び出し側はその完了を待つだけの短い処理なので、`async` にしても得がなく、そのままにしている。
#[tauri::command]
pub fn set_shortcut(app: AppHandle, accelerator: String) -> Result<Settings, AppError> {
    let (store, _) = services(&app)?;
    let settings = change_shortcut(&TauriRegistrar::new(app.clone()), &store, &accelerator)?;
    let _ = app.emit("settings-changed", settings.clone());
    Ok(settings)
}

/// ショートカットの入力待ちの間だけ、いまのショートカットを解除する(押したキーを画面に届けるため)。
#[tauri::command]
pub fn suspend_shortcut(app: AppHandle) -> Result<(), AppError> {
    crate::settings::shortcut::suspend(&app, &services(&app)?.0)
}

/// 解除していたショートカットを戻す。解除していなければ何もしない。
#[tauri::command]
pub fn resume_shortcut(app: AppHandle) -> Result<(), AppError> {
    crate::settings::shortcut::resume(&app, &services(&app)?.0)
}

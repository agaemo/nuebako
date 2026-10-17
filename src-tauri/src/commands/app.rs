//! アプリの状態、ウィンドウ操作、OS の設定を開く。

use tauri::{AppHandle, Manager};

use crate::app_state::AppState;
use crate::capture::health::capture_status;
use crate::domain::dto::{AppStatus, CaptureStatus, StorageStatus};
use crate::error::AppError;
use crate::setup::CaptureHandle;
use crate::{paste, window};

/// 履歴または設定の内部状態が壊れた可能性があるときに、画面に出す理由。
const POISONED_MESSAGE: &str =
    "内部の状態に異常がありました。履歴の保存に失敗する場合は、アプリを再起動してください";

/// 起動時の保存の状態に、実行中に起きた内部状態の異常を重ねる。
/// 起動時に問題があればそちらの理由を優先し、問題が無かったときだけ異常を `Error` として知らせる。
fn effective_storage(
    status: StorageStatus,
    message: Option<String>,
    poisoned: bool,
) -> (StorageStatus, Option<String>) {
    if status == StorageStatus::Ok && poisoned {
        (StorageStatus::Error, Some(POISONED_MESSAGE.to_string()))
    } else {
        (status, message)
    }
}

/// 起動状態、OS、権限、記録の状態を返す。状態が未登録のときは保存を `ok` として返す。
#[tauri::command]
pub fn get_app_status(app: AppHandle) -> Result<AppStatus, AppError> {
    let (storage, storage_message, guide_dismissed, settings_recovered) =
        match app.try_state::<AppState>() {
            Some(state) => {
                let (status, message) = effective_storage(
                    state.storage.0,
                    state.storage.1.clone(),
                    state.history.is_poisoned() || state.settings.is_poisoned(),
                );
                (
                    status,
                    message,
                    state.settings.get().permission_guide_dismissed,
                    state.settings.recovered(),
                )
            }
            None => (StorageStatus::Ok, None, false, false),
        };
    let capture = match app.try_state::<CaptureHandle>() {
        Some(h) => capture_status(h.source.status(), &h.health),
        None => CaptureStatus::Ok,
    };
    Ok(AppStatus {
        os: std::env::consts::OS.to_string(),
        version: app.package_info().version.to_string(),
        storage,
        storage_message,
        capture,
        settings_recovered,
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
/// `open` コマンドの終了まで待つ。待たないと、終了した子プロセスが回収されずにゾンビとして残る。
#[tauri::command]
pub fn open_accessibility_settings() -> Result<(), AppError> {
    #[cfg(target_os = "macos")]
    {
        let status = std::process::Command::new("open")
            .arg("x-apple.systempreferences:com.apple.preference.security?Privacy_Accessibility")
            .status()
            .map_err(|e| AppError::Internal(e.to_string()))?;
        if !status.success() {
            return Err(AppError::Internal(
                "システム設定を開けませんでした".to_string(),
            ));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn 内部状態の異常があり起動時は正常のとき保存をエラーにして理由を出す() {
        let (status, message) = effective_storage(StorageStatus::Ok, None, true);
        assert_eq!(status, StorageStatus::Error);
        assert_eq!(message.as_deref(), Some(POISONED_MESSAGE));
    }

    #[test]
    fn 内部状態の異常が無いとき起動時の状態をそのまま返す() {
        assert_eq!(
            effective_storage(StorageStatus::Ok, None, false),
            (StorageStatus::Ok, None)
        );
    }

    #[test]
    fn 起動時に問題があったときはその理由を優先する() {
        let (status, message) =
            effective_storage(StorageStatus::KeyUnavailable, Some("鍵".to_string()), true);
        assert_eq!(status, StorageStatus::KeyUnavailable);
        assert_eq!(message.as_deref(), Some("鍵"));
    }

    #[test]
    fn 状態のキーはsnake_caseで記録の状態と設定の復旧を含む() {
        let status = AppStatus {
            os: "macos".into(),
            version: "0.1.0".into(),
            storage: StorageStatus::Ok,
            storage_message: None,
            capture: CaptureStatus::Stopped,
            settings_recovered: true,
            paste_permission: true,
            permission_guide_dismissed: false,
        };
        let json = serde_json::to_value(status).unwrap();
        assert_eq!(json["capture"], "stopped");
        assert_eq!(json["settings_recovered"], true);
        assert_eq!(json["storage"], "ok");
    }
}

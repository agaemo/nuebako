//! 履歴ウィンドウ(history)と設定ウィンドウ(settings)の表示・非表示。

use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};
use tauri::{AppHandle, Emitter, Manager};

pub const HISTORY: &str = "history";
pub const SETTINGS: &str = "settings";

/// 履歴ウィンドウを最後に表示した時刻(ミリ秒)。表示直後のフォーカス変動で閉じないために使う。
static SHOWN_AT_MS: AtomicU64 = AtomicU64::new(0);

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// 履歴ウィンドウを前面に出し、検索欄にフォーカスを当てる合図を送る。
pub fn show_history(app: &AppHandle) {
    let Some(window) = app.get_webview_window(HISTORY) else {
        return;
    };
    crate::platform::focus::remember_frontmost();
    SHOWN_AT_MS.store(now_ms(), Ordering::SeqCst);
    let _ = window.center();
    let _ = window.show();
    let _ = window.set_focus();
    let _ = app.emit("history-window-shown", ());
}

/// 履歴ウィンドウを閉じたとき、元のアプリにフォーカスを戻すか。
/// 復帰はアプリごと隠す(macOS)ため、設定ウィンドウが表示中なら復帰しない(設定ウィンドウまで消えてしまう)。
fn should_restore_focus(settings_visible: bool) -> bool {
    !settings_visible
}

/// 履歴ウィンドウを隠し、(設定ウィンドウが無ければ)元のアプリにフォーカスを戻す。
pub fn hide_history(app: &AppHandle) {
    let Some(window) = app.get_webview_window(HISTORY) else {
        return;
    };
    if window.is_visible().unwrap_or(false) {
        let _ = window.hide();
        let settings_visible = app
            .get_webview_window(SETTINGS)
            .and_then(|w| w.is_visible().ok())
            .unwrap_or(false);
        if should_restore_focus(settings_visible) {
            crate::platform::focus::restore_previous(app);
        }
    }
}

/// ウィンドウのフォーカスが外れたときの処理。表示した直後の変動は無視する。
pub fn on_history_blur(app: &AppHandle) {
    if now_ms().saturating_sub(SHOWN_AT_MS.load(Ordering::SeqCst)) < 400 {
        return;
    }
    hide_history(app);
}

/// 履歴ウィンドウの表示と非表示を切り替える。
pub fn toggle_history(app: &AppHandle) {
    let visible = app
        .get_webview_window(HISTORY)
        .and_then(|w| w.is_visible().ok())
        .unwrap_or(false);
    if visible {
        hide_history(app);
    } else {
        show_history(app);
    }
}

/// 設定ウィンドウを表示する。履歴ウィンドウが開いていれば、フォーカスの復帰をせずに隠す。
pub fn show_settings(app: &AppHandle) {
    if let Some(history) = app.get_webview_window(HISTORY)
        && history.is_visible().unwrap_or(false)
    {
        let _ = history.hide();
    }
    if let Some(window) = app.get_webview_window(SETTINGS) {
        let _ = window.show();
        let _ = window.set_focus();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn 設定ウィンドウが表示中のとき履歴を閉じても元のアプリへ復帰しない() {
        // 復帰(アプリごと隠す)すると、表示中の設定ウィンドウまで消えてしまう
        assert!(!should_restore_focus(true));
    }

    #[test]
    fn 設定ウィンドウが無いとき履歴を閉じたら元のアプリへ復帰する() {
        assert!(should_restore_focus(false));
    }
}

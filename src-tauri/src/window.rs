//! 履歴ウィンドウ(history)と設定ウィンドウ(settings)の表示・非表示。

use std::sync::atomic::{AtomicI64, Ordering};
use tauri::{AppHandle, Emitter, Manager};

use crate::history::clock::{Clock, SystemClock};

pub const HISTORY: &str = "history";
pub const SETTINGS: &str = "settings";

/// 履歴ウィンドウを最後に表示した時刻(ミリ秒)。表示直後のフォーカス変動で閉じないために使う。
static SHOWN_AT_MS: AtomicI64 = AtomicI64::new(0);

/// 表示した直後に、フォーカスが外れたという通知が(ウィンドウを前面に出す途中で)届くことがある。
/// それで閉じてしまわないよう、この時間は無視する。
const BLUR_GRACE_MS: i64 = 400;

/// 表示してから間もなく、フォーカス喪失を無視すべきか。
fn is_within_blur_grace(now_ms: i64, shown_at_ms: i64) -> bool {
    now_ms.saturating_sub(shown_at_ms) < BLUR_GRACE_MS
}

/// 履歴ウィンドウを前面に出し、検索欄にフォーカスを当てる合図を送る。
pub fn show_history(app: &AppHandle) {
    let Some(window) = app.get_webview_window(HISTORY) else {
        return;
    };
    crate::platform::focus::remember_frontmost();
    SHOWN_AT_MS.store(SystemClock.now_ms(), Ordering::SeqCst);
    let _ = window.center();
    let _ = window.show();
    let _ = window.set_focus();
    let _ = app.emit("history-window-shown", ());
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
        // 復帰はアプリごと隠す(macOS)ため、設定ウィンドウが表示中なら復帰しない(設定ウィンドウまで消えてしまう)
        if !settings_visible {
            crate::platform::focus::restore_previous(app);
        }
    }
}

/// ウィンドウのフォーカスが外れたときの処理。表示した直後の変動は無視する。
pub fn on_history_blur(app: &AppHandle) {
    if is_within_blur_grace(SystemClock.now_ms(), SHOWN_AT_MS.load(Ordering::SeqCst)) {
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
    fn 表示の直後はフォーカス喪失を無視し_猶予を過ぎたら閉じる() {
        assert!(is_within_blur_grace(1_000, 1_000));
        assert!(is_within_blur_grace(1_399, 1_000));
        assert!(!is_within_blur_grace(1_400, 1_000));
    }

    #[test]
    fn 時計が戻っていても猶予の判定でパニックしない() {
        assert!(is_within_blur_grace(0, 1_000), "戻った時刻は直後として扱う");
        assert!(is_within_blur_grace(i64::MIN, i64::MAX));
    }
}

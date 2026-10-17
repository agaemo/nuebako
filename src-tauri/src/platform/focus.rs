//! 履歴ウィンドウの前面化と、元のアプリへのフォーカス復帰(ADR-003)。
//!
//! - macOS: アプリを Accessory にして Dock に出さない。ウィンドウを閉じるときに `app.hide()` で
//!   アプリごと隠すと、OS が直前のアプリにフォーカスを戻す。
//! - Windows: 履歴ウィンドウを開く前の最前面ウィンドウを記録し、閉じた後に戻す。

use tauri::AppHandle;

/// アプリの起動時に、macOS で Dock に出さない設定にする。
pub fn setup_activation(app: &AppHandle) {
    #[cfg(target_os = "macos")]
    let _ = app.set_activation_policy(tauri::ActivationPolicy::Accessory);
    #[cfg(not(target_os = "macos"))]
    let _ = app;
}

/// 履歴ウィンドウを開く前に、最前面のウィンドウを記録する(Windows のみ。macOS は OS が復帰させる)。
pub fn remember_frontmost() {
    #[cfg(windows)]
    win::remember();
}

/// 履歴ウィンドウを閉じた後、元のアプリにフォーカスを戻す。
pub fn restore_previous(app: &AppHandle) {
    #[cfg(target_os = "macos")]
    let _ = app.hide();
    #[cfg(windows)]
    {
        // Windows では AppHandle を使わない(直前のウィンドウのハンドルを記録しているため)
        let _ = app;
        win::restore();
    }
    #[cfg(not(any(target_os = "macos", windows)))]
    let _ = app;
}

#[cfg(windows)]
mod win {
    use std::sync::atomic::{AtomicIsize, Ordering};

    use windows::Win32::Foundation::HWND;
    use windows::Win32::UI::WindowsAndMessaging::{
        GetForegroundWindow, IsWindow, SetForegroundWindow,
    };

    /// 直前の最前面ウィンドウ(ハンドルを整数で保持する。0 は未記録)。
    static PREVIOUS: AtomicIsize = AtomicIsize::new(0);

    pub fn remember() {
        // SAFETY: 引数なしで、現在の最前面ウィンドウのハンドルを返すだけの API
        let hwnd = unsafe { GetForegroundWindow() };
        PREVIOUS.store(hwnd.0 as isize, Ordering::SeqCst);
    }

    pub fn restore() {
        let raw = PREVIOUS.load(Ordering::SeqCst);
        if raw == 0 {
            return;
        }
        let hwnd = HWND(raw as *mut core::ffi::c_void);
        // SAFETY: ハンドルが有効なウィンドウかを確認してから、最前面にするだけ
        unsafe {
            if IsWindow(Some(hwnd)).as_bool() {
                let _ = SetForegroundWindow(hwnd);
            }
        }
    }
}

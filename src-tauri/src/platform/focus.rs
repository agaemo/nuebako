//! 履歴ウィンドウの前面化と、元のアプリへのフォーカス復帰(ADR-003)。
//!
//! - macOS: アプリを Accessory にして Dock に出さない。ウィンドウを閉じるときに `app.hide()` で
//!   アプリごと隠すと、OS が直前のアプリにフォーカスを戻す。
//! - Windows: 履歴ウィンドウを開く前の最前面ウィンドウを記録し、閉じた後に戻す。貼り付けの前に、
//!   その記録と現在の最前面が一致するかも確かめる(`paste::target`)。

use tauri::AppHandle;

/// アプリの起動時に、macOS で Dock に出さない設定にする。
pub fn setup_activation(app: &AppHandle) {
    #[cfg(target_os = "macos")]
    let _ = app.set_activation_policy(tauri::ActivationPolicy::Accessory);
    #[cfg(not(target_os = "macos"))]
    let _ = app;
}

/// 履歴ウィンドウを開く前に、最前面のウィンドウを記録する(Windows のみ。macOS は OS が復帰させる)。
/// 自アプリのウィンドウとタスクバーは、貼り付け先になれないので記録しない(記録済みのものは消す)。
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

/// 記録した貼り付け先のウィンドウ(Windows)。未記録は 0。
#[cfg(windows)]
pub fn recorded_destination() -> isize {
    win::recorded()
}

/// 現在の最前面のウィンドウ(Windows)。
#[cfg(windows)]
pub fn current_foreground() -> isize {
    win::foreground()
}

/// Alt・Shift・Ctrl のどれかが押されているか(Windows)。
#[cfg(windows)]
pub fn any_modifier_down() -> bool {
    win::any_modifier_down()
}

#[cfg(windows)]
mod win {
    use std::sync::atomic::{AtomicIsize, Ordering};

    use ::windows::Win32::Foundation::HWND;
    use ::windows::Win32::System::Threading::GetCurrentProcessId;
    use ::windows::Win32::UI::WindowsAndMessaging::{
        GetClassNameW, GetForegroundWindow, GetWindowThreadProcessId, IsWindow, SetForegroundWindow,
    };

    use crate::paste::target::is_recordable_destination;

    /// 直前の最前面ウィンドウ(ハンドルを整数で保持する。0 は未記録)。
    static PREVIOUS: AtomicIsize = AtomicIsize::new(0);

    // `GetAsyncKeyState` は windows crate の `Win32_UI_Input_KeyboardAndMouse` に入っているが、
    // その機能を足すと Cargo.toml の変更になるため、user32 の関数を直接宣言する。
    #[link(name = "user32")]
    unsafe extern "system" {
        fn GetAsyncKeyState(vkey: i32) -> i16;
    }

    /// 仮想キーコード(Shift・Ctrl・Alt)。
    const VK_SHIFT: i32 = 0x10;
    const VK_CONTROL: i32 = 0x11;
    const VK_MENU: i32 = 0x12;

    pub fn any_modifier_down() -> bool {
        [VK_SHIFT, VK_CONTROL, VK_MENU].into_iter().any(|vk| {
            // SAFETY: 仮想キーコードを渡して、押下状態を読むだけの API
            let state = unsafe { GetAsyncKeyState(vk) };
            // 最上位ビットが「いま押されている」
            (state as u16) & 0x8000 != 0
        })
    }

    fn is_own_window(hwnd: HWND) -> bool {
        let mut pid = 0u32;
        // SAFETY: 有効なポインタ(pid)を渡す。hwnd が無効でも 0 が返るだけ
        unsafe { GetWindowThreadProcessId(hwnd, Some(&mut pid)) };
        // SAFETY: 引数なしで、自プロセスの ID を返すだけの API
        pid == unsafe { GetCurrentProcessId() }
    }

    fn class_name(hwnd: HWND) -> String {
        let mut buf = [0u16; 256];
        // SAFETY: バッファの長さを渡す。戻り値は書き込んだ文字数(失敗は 0)
        let len = unsafe { GetClassNameW(hwnd, &mut buf) };
        String::from_utf16_lossy(&buf[..len.max(0) as usize])
    }

    pub fn remember() {
        // SAFETY: 引数なしで、現在の最前面ウィンドウのハンドルを返すだけの API
        let hwnd = unsafe { GetForegroundWindow() };
        let raw = hwnd.0 as isize;
        let recordable = is_recordable_destination(raw, is_own_window(hwnd), &class_name(hwnd));
        PREVIOUS.store(if recordable { raw } else { 0 }, Ordering::SeqCst);
    }

    pub fn recorded() -> isize {
        PREVIOUS.load(Ordering::SeqCst)
    }

    pub fn foreground() -> isize {
        // SAFETY: 引数なしで、現在の最前面ウィンドウのハンドルを返すだけの API
        unsafe { GetForegroundWindow() }.0 as isize
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

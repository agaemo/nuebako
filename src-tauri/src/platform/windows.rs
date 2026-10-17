// 骨組み(P1-4)の属性。利用側の接続は P3-1 で行うため、残してよい。
#![allow(dead_code, unused_variables)]

//! Windows 固有の実装(`::windows` crate)。コピー元アプリ(クリップボードの所有者)と最前面、実行中アプリの取得。
//! このファイルでは、モジュール名との衝突を避けるため、crate は `::windows::...` で参照する。

use ::windows::Win32::Foundation::{CloseHandle, HWND, LPARAM};
use ::windows::Win32::System::DataExchange::GetClipboardOwner;
use ::windows::Win32::System::Threading::{
    OpenProcess, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION, QueryFullProcessImageNameW,
};
use ::windows::Win32::UI::WindowsAndMessaging::{
    EnumWindows, GetForegroundWindow, GetWindowTextLengthW, GetWindowThreadProcessId,
    IsWindowVisible,
};
use ::windows::core::{BOOL, PWSTR};

use crate::domain::clip::AppRef;
use crate::error::AppError;

/// exe のフルパスから、識別子(ファイル名。例: `Bitwarden.exe`)と表示名(拡張子なし)を作る。
fn app_ref_from_path(path: &str) -> Option<AppRef> {
    let file = path.rsplit(['\\', '/']).next()?;
    if file.is_empty() {
        return None;
    }
    let name = file
        .strip_suffix(".exe")
        .or_else(|| file.strip_suffix(".EXE"))
        .unwrap_or(file);
    Some(AppRef {
        id: file.to_string(),
        name: name.to_string(),
    })
}

/// プロセス ID から exe のフルパスを得る。
fn exe_path_of_pid(pid: u32) -> Option<String> {
    // SAFETY: 取得したハンドルは、このブロック内で必ず CloseHandle する。バッファは十分な長さを渡す
    unsafe {
        let handle = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid).ok()?;
        let mut buf = vec![0u16; 1024];
        let mut len = buf.len() as u32;
        let result = QueryFullProcessImageNameW(
            handle,
            PROCESS_NAME_WIN32,
            PWSTR(buf.as_mut_ptr()),
            &mut len,
        );
        let _ = CloseHandle(handle);
        result.ok()?;
        Some(String::from_utf16_lossy(&buf[..len as usize]))
    }
}

/// ウィンドウを所有するプロセスのアプリ情報。
fn app_of_window(hwnd: HWND) -> Option<AppRef> {
    if hwnd.0.is_null() {
        return None;
    }
    let mut pid = 0u32;
    // SAFETY: 有効なポインタ(pid)を渡す。hwnd が無効でも 0 が返るだけ
    unsafe { GetWindowThreadProcessId(hwnd, Some(&mut pid)) };
    if pid == 0 {
        return None;
    }
    app_ref_from_path(&exe_path_of_pid(pid)?)
}

/// クリップボードの所有者のアプリ。
pub fn clipboard_owner() -> Option<AppRef> {
    // SAFETY: 引数なしで、所有者のウィンドウハンドルを返すだけの API
    let hwnd = unsafe { GetClipboardOwner() }.ok()?;
    app_of_window(hwnd)
}

/// 最前面のウィンドウのアプリ。
pub fn frontmost_app() -> Option<AppRef> {
    // SAFETY: 引数なしで、最前面のウィンドウハンドルを返すだけの API
    let hwnd = unsafe { GetForegroundWindow() };
    app_of_window(hwnd)
}

/// 画面に出ているトップレベルウィンドウを持つアプリの一覧(名前順、重複なし)。
pub fn running_apps() -> Result<Vec<AppRef>, AppError> {
    extern "system" fn collect(hwnd: HWND, lparam: LPARAM) -> BOOL {
        // SAFETY: lparam は呼び出し元が渡した `Vec<AppRef>` への有効な可変ポインタ(EnumWindows の間だけ使う)
        let out = unsafe { &mut *(lparam.0 as *mut Vec<AppRef>) };
        // SAFETY: ハンドルを調べるだけの API
        let (visible, title_len) =
            unsafe { (IsWindowVisible(hwnd).as_bool(), GetWindowTextLengthW(hwnd)) };
        if visible
            && title_len > 0
            && let Some(app) = app_of_window(hwnd)
        {
            out.push(app);
        }
        BOOL(1)
    }

    let mut apps: Vec<AppRef> = Vec::new();
    // SAFETY: コールバックは EnumWindows の呼び出し中だけ、`apps` を可変で参照する
    unsafe {
        EnumWindows(
            Some(collect),
            LPARAM(&mut apps as *mut Vec<AppRef> as isize),
        )
    }
    .map_err(|e| AppError::Internal(e.to_string()))?;
    apps.sort_by_key(|a| a.name.to_lowercase());
    apps.dedup_by(|a, b| a.id.eq_ignore_ascii_case(&b.id));
    Ok(apps)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exeのフルパスからファイル名と表示名を作る() {
        let app = app_ref_from_path("C:\\Program Files\\Bitwarden\\Bitwarden.exe").unwrap();
        assert_eq!(app.id, "Bitwarden.exe");
        assert_eq!(app.name, "Bitwarden");
    }

    #[test]
    fn 空のパスのときnoneになる() {
        assert!(app_ref_from_path("").is_none());
        assert!(app_ref_from_path("C:\\dir\\").is_none());
    }
}

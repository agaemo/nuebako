//! OS ごとの違いを吸収する層。

pub mod apps;
pub mod focus;
#[cfg(target_os = "macos")]
pub mod macos;
// 注意: このモジュールの中で windows crate を使うときは、`::windows::...` と書く(モジュール名と衝突するため)
#[cfg(windows)]
pub mod windows;

use crate::domain::clip::AppRef;
use crate::error::AppError;

/// 実行中のアプリやコピー元の取得。
pub trait AppInspector: Send + Sync {
    /// いま最前面のアプリ
    fn frontmost_app(&self) -> Option<AppRef>;
    /// 実行中のアプリの一覧(除外アプリの追加用)
    fn running_apps(&self) -> Result<Vec<AppRef>, AppError>;
    /// クリップボードの所有者(Windows の `GetClipboardOwner`)。取れなければ `None`
    fn clipboard_owner(&self) -> Option<AppRef>;
}

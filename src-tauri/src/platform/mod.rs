//! OS ごとの違いを吸収する層。

// 骨組み(P1-4)。利用側は各トラックが実装するため、P3-1 でこの属性を外す。
#![allow(dead_code)]

pub mod apps;
pub mod focus;
#[cfg(target_os = "macos")]
pub mod macos;
// 注意: このモジュールの中で windows crate を使うときは、`::windows::...` と書く(モジュール名と衝突するため)
#[cfg(windows)]
pub mod windows;

use crate::domain::clip::AppRef;
use crate::error::AppError;

/// 履歴ウィンドウの前面化と、元のアプリへのフォーカス復帰(ADR-003)。
pub trait FocusController: Send + Sync {
    /// 履歴ウィンドウを開く前に、最前面のウィンドウを記録する。
    fn remember(&self);
    /// ウィンドウを閉じた後、元のアプリにフォーカスを戻す。
    fn restore(&self);
}

/// 実行中のアプリやコピー元の取得。
pub trait AppInspector: Send + Sync {
    /// いま最前面のアプリ
    fn frontmost_app(&self) -> Option<AppRef>;
    /// 実行中のアプリの一覧(除外アプリの追加用)
    fn running_apps(&self) -> Result<Vec<AppRef>, AppError>;
    /// クリップボードの所有者(Windows の `GetClipboardOwner`)。取れなければ `None`
    fn clipboard_owner(&self) -> Option<AppRef>;
}

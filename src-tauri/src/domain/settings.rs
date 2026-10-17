//! 設定の型と既定値。

use serde::{Deserialize, Serialize};

use super::clip::AppRef;

/// テーマ。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Theme {
    System,
    Light,
    Dark,
}

/// 上限の許容範囲(これを超える値は検証でエラーにする)。
pub const MAX_ITEMS_LIMIT: u32 = 100_000;
pub const RETENTION_DAYS_LIMIT: u32 = 3_650;

/// 件数の上限と保存期間の既定値。
pub const DEFAULT_MAX_ITEMS: u32 = 1_000;
pub const DEFAULT_RETENTION_DAYS: u32 = 30;

/// 利用者の設定。`settings.json` に平文で保存する(機密は含まない)。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    /// 履歴ウィンドウを呼び出すショートカット(例: `CommandOrControl+Shift+V`)
    pub shortcut: String,
    /// 保存する最大件数
    pub max_items: u32,
    /// 保存期間(日)
    pub retention_days: u32,
    pub theme: Theme,
    /// 記録しないアプリ
    pub excluded_apps: Vec<AppRef>,
    /// コピー元が分からないときに記録するか
    pub record_unknown_source: bool,
    /// 自動ペーストの権限の案内を、非表示にしたか
    pub permission_guide_dismissed: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            shortcut: "CommandOrControl+Shift+V".to_string(),
            max_items: DEFAULT_MAX_ITEMS,
            retention_days: DEFAULT_RETENTION_DAYS,
            theme: Theme::System,
            excluded_apps: default_excluded_apps(),
            record_unknown_source: true,
            permission_guide_dismissed: false,
        }
    }
}

/// 既定の除外アプリ(パスワード管理アプリ)。識別子は実機のパスワード管理アプリで未確認。
pub fn default_excluded_apps() -> Vec<AppRef> {
    let app = |id: &str, name: &str| AppRef {
        id: id.to_string(),
        name: name.to_string(),
    };
    #[cfg(target_os = "macos")]
    {
        vec![
            app("com.1password.1password", "1Password"),
            app("com.bitwarden.desktop", "Bitwarden"),
            app("org.keepassxc.keepassxc", "KeePassXC"),
        ]
    }
    #[cfg(not(target_os = "macos"))]
    {
        vec![
            app("1Password.exe", "1Password"),
            app("Bitwarden.exe", "Bitwarden"),
            app("KeePassXC.exe", "KeePassXC"),
        ]
    }
}

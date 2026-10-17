// 骨組み(P1-4)の属性。利用側の接続は P3-1 で行うため、残してよい。
#![allow(dead_code, unused_variables)]

//! macOS 固有の実装(objc2 / objc2-app-kit)。コピー元アプリの取得と、実行中アプリの一覧。

use objc2_app_kit::{NSApplicationActivationPolicy, NSRunningApplication, NSWorkspace};

use crate::domain::clip::AppRef;

/// 実行中アプリから、識別子(bundle id)と表示名を取り出す。bundle id が無いアプリは `None`。
fn to_app_ref(app: &NSRunningApplication) -> Option<AppRef> {
    let id = app.bundleIdentifier()?.to_string();
    if id.is_empty() {
        return None;
    }
    let name = app
        .localizedName()
        .map(|n| n.to_string())
        .filter(|n| !n.is_empty())
        .unwrap_or_else(|| id.clone());
    Some(AppRef { id, name })
}

/// いま最前面のアプリ。取得できない、または bundle id が空のときは `None`。
pub fn frontmost_app() -> Option<AppRef> {
    let workspace = NSWorkspace::sharedWorkspace();
    let app = workspace.frontmostApplication()?;
    to_app_ref(&app)
}

/// 実行中のアプリの一覧(通常のアプリ = activationPolicy が Regular のものだけ)。名前順、重複なし。
pub fn running_apps() -> Vec<AppRef> {
    let workspace = NSWorkspace::sharedWorkspace();
    let mut apps: Vec<AppRef> = workspace
        .runningApplications()
        .iter()
        .filter(|a| a.activationPolicy() == NSApplicationActivationPolicy::Regular)
        .filter_map(|a| to_app_ref(&a))
        .collect();
    apps.sort_by_key(|a| a.name.to_lowercase());
    apps.dedup_by(|a, b| a.id == b.id);
    apps
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 実機の NSWorkspace に問い合わせる。
    #[test]
    #[ignore]
    fn 実機で実行中のアプリの一覧が取れる() {
        let apps = running_apps();
        assert!(!apps.is_empty());
        assert!(apps.iter().all(|a| !a.id.is_empty()));
    }
}

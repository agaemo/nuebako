//! コピー元アプリと実行中アプリの取得(OS 別の実装を束ねる)。track-clipboard が実装する。

use super::AppInspector;
use crate::domain::clip::AppRef;
use crate::error::AppError;

/// このアプリ自身の識別子(macOS の bundle id)。最前面がこれのときは、コピー元不明として扱う。
pub const SELF_APP_ID: &str = "io.github.agaemo.nuebako";

/// 自分自身、または識別子が空のアプリは、コピー元として使えない。
fn usable(app: Option<AppRef>) -> Option<AppRef> {
    app.filter(|a| !a.id.is_empty() && !a.id.eq_ignore_ascii_case(SELF_APP_ID))
}

/// コピー元アプリを決める。優先順: クリップボード自身の申告(`source_hint`)、
/// クリップボードの所有者(Windows)、最前面のアプリ。
/// 自分自身や識別子が空のものは飛ばし、どれも使えなければ `None`(コピー元不明)。
pub fn resolve_source(
    source_hint: Option<AppRef>,
    owner: Option<AppRef>,
    frontmost: Option<AppRef>,
) -> Option<AppRef> {
    usable(source_hint)
        .or_else(|| usable(owner))
        .or_else(|| usable(frontmost))
}

/// OS に問い合わせる実装。
pub struct SystemAppInspector;

impl AppInspector for SystemAppInspector {
    fn frontmost_app(&self) -> Option<AppRef> {
        #[cfg(target_os = "macos")]
        return super::macos::frontmost_app();
        #[cfg(windows)]
        return super::windows::frontmost_app();
        #[cfg(not(any(target_os = "macos", windows)))]
        None
    }

    fn running_apps(&self) -> Result<Vec<AppRef>, AppError> {
        #[cfg(target_os = "macos")]
        return Ok(super::macos::running_apps());
        #[cfg(windows)]
        return super::windows::running_apps();
        #[cfg(not(any(target_os = "macos", windows)))]
        Ok(Vec::new())
    }

    fn clipboard_owner(&self) -> Option<AppRef> {
        #[cfg(windows)]
        return super::windows::clipboard_owner();
        #[cfg(not(windows))]
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn app(id: &str) -> Option<AppRef> {
        Some(AppRef {
            id: id.to_string(),
            name: id.to_string(),
        })
    }

    #[test]
    fn 申告があるときそれをコピー元にする() {
        let r = resolve_source(app("a"), app("b"), app("c"));
        assert_eq!(r.unwrap().id, "a");
    }

    #[test]
    fn 申告がなければ所有者を使う() {
        let r = resolve_source(None, app("b"), app("c"));
        assert_eq!(r.unwrap().id, "b");
    }

    #[test]
    fn 申告も所有者もなければ最前面を使う() {
        let r = resolve_source(None, None, app("c"));
        assert_eq!(r.unwrap().id, "c");
    }

    #[test]
    fn 最前面が自分自身のときコピー元不明にする() {
        assert!(resolve_source(None, None, app(SELF_APP_ID)).is_none());
        assert!(resolve_source(None, None, app("IO.GITHUB.AGAEMO.NUEBAKO")).is_none());
    }

    #[test]
    fn 識別子が空のときコピー元不明にする() {
        assert!(resolve_source(None, None, app("")).is_none());
    }

    #[test]
    fn 申告が自分自身のときは次の候補を使う() {
        let r = resolve_source(app(SELF_APP_ID), None, app("c"));
        assert_eq!(r.unwrap().id, "c");
    }

    #[test]
    fn 候補がなければコピー元不明にする() {
        assert!(resolve_source(None, None, None).is_none());
    }
}

//! クリップボードへの書き込み(clipboard-rs を trait で包む)。

use clipboard_rs::common::RustImage;
use clipboard_rs::{Clipboard, ClipboardContent, ClipboardContext, RustImageData};

use crate::domain::clip::ClipContent;
use crate::error::AppError;

pub trait ClipboardWriter: Send + Sync {
    /// 内容をクリップボードへ書く(全体を置き換える)。
    fn write(&self, content: &ClipContent) -> Result<(), AppError>;
}

/// 他のクリップボード監視(OS のクリップボード履歴、クラウド同期、他のアプリ)に、記録させないための印。
/// 履歴から貼り付けた内容が、再びどこかへ複製されることを防ぐ。値は意味を持たない(形式があることが印)。
fn exclusion_markers() -> Vec<(&'static str, Vec<u8>)> {
    #[cfg(target_os = "macos")]
    return vec![("org.nspasteboard.TransientType", vec![0])];
    // Windows のクリップボード履歴(Win+V)とクラウド同期の対象外にする。後者は DWORD の 0
    #[cfg(windows)]
    return vec![
        (
            "ExcludeClipboardContentFromMonitorProcessing",
            vec![0, 0, 0, 0],
        ),
        ("CanIncludeInClipboardHistory", vec![0, 0, 0, 0]),
    ];
    #[cfg(not(any(target_os = "macos", windows)))]
    Vec::new()
}

/// 内容の項目に、除外の印を続ける。
/// 画像の書き込みは(Windows で)クリップボードを空にしてから行われるため、印は内容のあとに置く。
fn with_exclusion_markers(mut items: Vec<ClipboardContent>) -> Vec<ClipboardContent> {
    items.extend(
        exclusion_markers()
            .into_iter()
            .map(|(format, data)| ClipboardContent::Other(format.to_string(), data)),
    );
    items
}

/// clipboard-rs による書き込み。
///
/// `set_text` などは呼ぶたびにクリップボード全体を置き換えるため、`set(Vec<ClipboardContent>)` を使う。
/// 実際のクリップボードに触れるため、テストは `#[ignore]` の実機テストにする。
pub struct ClipboardRsWriter;

impl ClipboardWriter for ClipboardRsWriter {
    fn write(&self, content: &ClipContent) -> Result<(), AppError> {
        let ctx = ClipboardContext::new().map_err(|e| AppError::Clipboard(e.to_string()))?;
        let item = match content {
            ClipContent::Text(t) => ClipboardContent::Text(t.clone()),
            ClipContent::Image { png, .. } => {
                let image = RustImageData::from_bytes(png)
                    .map_err(|e| AppError::Clipboard(e.to_string()))?;
                ClipboardContent::Image(image)
            }
        };
        ctx.set(with_exclusion_markers(vec![item]))
            .map_err(|e| AppError::Clipboard(e.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn other_formats(items: &[ClipboardContent]) -> Vec<String> {
        items
            .iter()
            .filter_map(|c| match c {
                ClipboardContent::Other(f, _) => Some(f.clone()),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn 書き込む内容の項目は先頭のまま除外の印が続く() {
        let items = with_exclusion_markers(vec![ClipboardContent::Text("ダミー".into())]);
        assert!(matches!(&items[0], ClipboardContent::Text(t) if t == "ダミー"));
        assert_eq!(items.len(), 1 + exclusion_markers().len());
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn macosでは一時的な内容を示す型を付ける() {
        let items = with_exclusion_markers(vec![ClipboardContent::Text("x".into())]);
        assert_eq!(
            other_formats(&items),
            vec!["org.nspasteboard.TransientType".to_string()]
        );
    }

    #[cfg(windows)]
    #[test]
    fn windowsでは監視とクリップボード履歴の対象外を示す形式を付ける() {
        let items = with_exclusion_markers(vec![ClipboardContent::Text("x".into())]);
        let formats = other_formats(&items);
        assert!(formats.contains(&"ExcludeClipboardContentFromMonitorProcessing".to_string()));
        assert!(formats.contains(&"CanIncludeInClipboardHistory".to_string()));
        // CanIncludeInClipboardHistory は DWORD の 0
        assert!(items.iter().any(|c| matches!(c,
            ClipboardContent::Other(f, d) if f == "CanIncludeInClipboardHistory" && d == &vec![0, 0, 0, 0])));
    }

    #[test]
    fn 付ける印はこのアプリの機密判定が除外する形式と一致する() {
        // 自分が書いた内容を、自分の監視が(抑止の期限が切れた後でも)記録しないことを保証する
        let items = with_exclusion_markers(vec![]);
        let formats = other_formats(&items);
        assert!(crate::capture::filter::has_sensitive_format(
            &formats,
            &|_| Some(vec![0, 0, 0, 0])
        ));
    }

    /// 実際のクリップボードを書き換えるため、手元で明示的に実行する。
    #[test]
    #[ignore]
    fn 実機で書き込んだ内容に除外の印が付く() {
        ClipboardRsWriter
            .write(&ClipContent::Text("nuebako-marker-test".to_string()))
            .unwrap();
        let ctx = ClipboardContext::new().unwrap();
        let formats = ctx.available_formats().unwrap();
        for (marker, _) in exclusion_markers() {
            assert!(
                formats.iter().any(|f| f.eq_ignore_ascii_case(marker)),
                "{marker}: {formats:?}"
            );
        }
    }

    /// 実際のクリップボードを書き換えるため、手元で明示的に実行する。
    #[test]
    #[ignore]
    fn 実機でテキストを書き込むと読み戻せる() {
        use clipboard_rs::Clipboard;
        ClipboardRsWriter
            .write(&ClipContent::Text("nuebako-test".to_string()))
            .unwrap();
        let ctx = ClipboardContext::new().unwrap();
        assert_eq!(ctx.get_text().unwrap(), "nuebako-test");
    }
}

// 骨組み(P1-4)。本体は各トラックで実装し、P3-1 でこの属性を外す。
#![allow(dead_code, unused_variables)]

//! クリップボードへの書き込み(clipboard-rs を trait で包む)。

use clipboard_rs::common::RustImage;
use clipboard_rs::{Clipboard, ClipboardContent, ClipboardContext, RustImageData};

use crate::domain::clip::ClipContent;
use crate::error::AppError;

pub trait ClipboardWriter: Send + Sync {
    /// 内容をクリップボードへ書く(全体を置き換える)。
    fn write(&self, content: &ClipContent) -> Result<(), AppError>;
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
        ctx.set(vec![item])
            .map_err(|e| AppError::Clipboard(e.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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

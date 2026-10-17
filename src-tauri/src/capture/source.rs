// 骨組み(P1-4)。本体は各トラックで実装し、P3-1 でこの属性を外す。
#![allow(dead_code, unused_variables)]

//! クリップボードの監視(clipboard-rs を trait で包む)。

use crate::domain::clip::AppRef;
use crate::error::AppError;

/// ある時点のクリップボードの内容。
pub struct ClipboardSnapshot {
    /// 利用できる形式の名前(機密の型名の判定に使う)
    pub formats: Vec<String>,
    pub text: Option<String>,
    pub image_png: Option<Vec<u8>>,
    /// `org.nspasteboard.source` などから分かったコピー元
    pub source_hint: Option<AppRef>,
}

pub trait ClipboardSource: Send + Sync {
    /// 変更の通知を受けて `on_change` を呼ぶ監視を、専用スレッドで始める(呼び出しはブロックしない)。
    fn start(&self, on_change: Box<dyn Fn() + Send + Sync>) -> Result<(), AppError>;
    /// 監視を止める。
    fn stop(&self);
    /// いまの内容を読む。読めない内容(不正な UTF-16 など)は、該当部分を `None` にする。
    fn snapshot(&self) -> Result<ClipboardSnapshot, AppError>;
}

#[cfg(test)]
mod tests {
    #[test]
    fn compiled() {}
}

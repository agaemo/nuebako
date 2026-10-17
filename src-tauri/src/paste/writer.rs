// 骨組み(P1-4)。本体は各トラックで実装し、P3-1 でこの属性を外す。
#![allow(dead_code, unused_variables)]

//! クリップボードへの書き込み(clipboard-rs を trait で包む)。

use crate::domain::clip::ClipContent;
use crate::error::AppError;

pub trait ClipboardWriter: Send + Sync {
    /// 内容をクリップボードへ書く(全体を置き換える)。
    fn write(&self, content: &ClipContent) -> Result<(), AppError>;
}

#[cfg(test)]
mod tests {
    #[test]
    fn compiled() {}
}

// 骨組み(P1-4)。本体は各トラックで実装し、P3-1 でこの属性を外す。
#![allow(dead_code, unused_variables)]

//! 設定の入力検証。track-settings が実装する。

use crate::error::AppError;

/// 最大件数と保存期間(文字列)を検証して数値にする。
pub fn validate_limits(max_items: &str, retention_days: &str) -> Result<(u32, u32), AppError> {
    unimplemented!("track-settings")
}

#[cfg(test)]
mod tests {
    #[test]
    fn compiled() {}
}

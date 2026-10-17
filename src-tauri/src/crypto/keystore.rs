// 骨組み(P1-4)。本体は各トラックで実装し、P3-1 でこの属性を外す。
#![allow(dead_code, unused_variables)]

//! 鍵の保管。OS のキーチェーンを使う実装は track-store が書く。

use crate::error::AppError;

pub trait KeyStore: Send + Sync {
    /// 保存済みの鍵を返す。無ければ `None`。取得に失敗したらエラー。
    fn load(&self) -> Result<Option<[u8; 32]>, AppError>;
    /// 鍵を保存する。
    fn save(&self, key: &[u8; 32]) -> Result<(), AppError>;
}

#[cfg(test)]
mod tests {
    #[test]
    fn compiled() {}
}

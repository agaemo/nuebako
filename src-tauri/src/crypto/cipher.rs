// 骨組み(P1-4)。本体は各トラックで実装し、P3-1 でこの属性を外す。
#![allow(dead_code, unused_variables)]

//! AES-256-GCM による1件ずつの暗号化。track-store が実装する。

use crate::error::AppError;

pub struct Cipher {
    _private: (),
}

impl Cipher {
    /// 32 バイトの鍵から作る。
    pub fn new(key: [u8; 32]) -> Self {
        unimplemented!("track-store")
    }

    /// 暗号化する。戻り値は nonce(12 バイト)と暗号文を連結したもの。
    pub fn encrypt(&self, aad: &[u8], plain: &[u8]) -> Result<Vec<u8>, AppError> {
        unimplemented!("track-store")
    }

    /// 復号する。鍵や追加データが違う、改ざんされている場合はエラー。
    pub fn decrypt(&self, aad: &[u8], data: &[u8]) -> Result<Vec<u8>, AppError> {
        unimplemented!("track-store")
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn compiled() {}
}

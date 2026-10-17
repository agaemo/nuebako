//! AES-256-GCM による1件ずつの暗号化。呼び出しごとに nonce を作り、呼び出し側が行の id を追加の認証データとして渡す。
//! 行の入れ替えや改ざんを、復号の失敗として検出するため。

use aes_gcm::aead::{Aead, AeadCore, Generate, Key, KeyInit, Payload};
use aes_gcm::{Aes256Gcm, Nonce};

use crate::error::AppError;

pub const NONCE_LEN: usize = 12;
/// GCM の認証タグの長さ
const TAG_LEN: usize = 16;

pub struct Cipher {
    inner: Aes256Gcm,
}

impl Cipher {
    /// 新しい 32 バイトの鍵を生成する。
    pub fn generate_key() -> [u8; 32] {
        let key = Key::<Aes256Gcm>::generate();
        let mut out = [0u8; 32];
        out.copy_from_slice(key.as_slice());
        out
    }

    /// 32 バイトの鍵から作る。
    pub fn new(key: [u8; 32]) -> Self {
        let key = Key::<Aes256Gcm>::try_from(&key[..]).expect("鍵は 32 バイト");
        Self {
            inner: Aes256Gcm::new(&key),
        }
    }

    /// 暗号化する。戻り値は nonce(12 バイト)と暗号文を連結したもの。
    /// nonce は呼び出しごとに乱数で生成する。
    pub fn encrypt(&self, aad: &[u8], plain: &[u8]) -> Result<Vec<u8>, AppError> {
        let nonce = Nonce::<<Aes256Gcm as AeadCore>::NonceSize>::generate();
        let ct = self
            .inner
            .encrypt(&nonce, Payload { msg: plain, aad })
            .map_err(|_| AppError::Storage("暗号化に失敗しました".to_string()))?;
        let mut out = Vec::with_capacity(NONCE_LEN + ct.len());
        out.extend_from_slice(nonce.as_slice());
        out.extend_from_slice(&ct);
        Ok(out)
    }

    /// 復号する。鍵や追加データが違う、改ざんされている場合はエラー。
    pub fn decrypt(&self, aad: &[u8], data: &[u8]) -> Result<Vec<u8>, AppError> {
        if data.len() < NONCE_LEN + TAG_LEN {
            return Err(AppError::Storage("暗号データの形式が不正です".to_string()));
        }
        let (nonce, ct) = data.split_at(NONCE_LEN);
        let nonce = Nonce::<<Aes256Gcm as AeadCore>::NonceSize>::try_from(nonce)
            .map_err(|_| AppError::Storage("暗号データの形式が不正です".to_string()))?;
        self.inner
            .decrypt(&nonce, Payload { msg: ct, aad })
            .map_err(|_| AppError::Storage("復号に失敗しました".to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cipher() -> Cipher {
        Cipher::new([7u8; 32])
    }

    #[test]
    fn 暗号化して復号すると元に戻る() {
        let c = cipher();
        let enc = c.encrypt(b"aad-1", "ダミーの本文".as_bytes()).unwrap();
        assert_eq!(
            c.decrypt(b"aad-1", &enc).unwrap(),
            "ダミーの本文".as_bytes()
        );
    }

    #[test]
    fn 空の平文も暗号化して復号できる() {
        let c = cipher();
        let enc = c.encrypt(b"a", b"").unwrap();
        assert_eq!(c.decrypt(b"a", &enc).unwrap(), b"");
    }

    #[test]
    fn 同じ平文でもnonceが毎回異なる() {
        let c = cipher();
        let a = c.encrypt(b"a", b"same").unwrap();
        let b = c.encrypt(b"a", b"same").unwrap();
        assert_ne!(a[..NONCE_LEN], b[..NONCE_LEN]);
        assert_ne!(a, b);
    }

    #[test]
    fn 暗号文は平文を含まない() {
        let c = cipher();
        let enc = c.encrypt(b"a", b"plain-dummy-text").unwrap();
        assert!(!enc.windows(10).any(|w| w == b"plain-dumm"));
    }

    #[test]
    fn aadが違うと復号に失敗する() {
        let c = cipher();
        let enc = c.encrypt(b"nuebako:clips:v1:1", b"x").unwrap();
        assert!(c.decrypt(b"nuebako:clips:v1:2", &enc).is_err());
    }

    #[test]
    fn 暗号文を1ビット改ざんすると復号に失敗する() {
        let c = cipher();
        let enc = c.encrypt(b"a", b"hello").unwrap();
        for pos in [0, NONCE_LEN, enc.len() - 1] {
            let mut bad = enc.clone();
            bad[pos] ^= 1;
            assert!(c.decrypt(b"a", &bad).is_err(), "pos={pos}");
        }
    }

    #[test]
    fn 違う鍵では復号に失敗する() {
        let enc = cipher().encrypt(b"a", b"hello").unwrap();
        assert!(Cipher::new([8u8; 32]).decrypt(b"a", &enc).is_err());
    }

    #[test]
    fn 短すぎるデータは復号に失敗する() {
        assert!(cipher().decrypt(b"a", &[0u8; 5]).is_err());
    }

    #[test]
    fn 生成した鍵は毎回異なる() {
        assert_ne!(Cipher::generate_key(), Cipher::generate_key());
    }
}

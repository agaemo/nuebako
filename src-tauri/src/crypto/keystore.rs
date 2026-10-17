//! 暗号鍵の保管。鍵は履歴のファイルとは別に OS のキーチェーンへ置く。DB のファイルだけを持ち出されても、履歴を読めないようにするため。

use crate::crypto::cipher::Cipher;
use crate::error::AppError;

pub trait KeyStore: Send + Sync {
    /// 保存済みの鍵を返す。無ければ `None`。取得に失敗したらエラー。
    fn load(&self) -> Result<Option<[u8; 32]>, AppError>;
    /// 鍵を保存する。
    fn save(&self, key: &[u8; 32]) -> Result<(), AppError>;
}

/// 鍵を取得する。無ければ生成して保存する。
/// 取得に失敗した場合は、新しい鍵を作らずにエラーを返す(既存の履歴を読めなくしないため)。
/// 「DB が既にあるのに鍵が無い」場合は、この関数ではなく `KeyStore::load` を直接使うこと。
pub fn load_or_create(store: &dyn KeyStore) -> Result<[u8; 32], AppError> {
    if let Some(key) = store.load()? {
        return Ok(key);
    }
    let key = Cipher::generate_key();
    store.save(&key)?;
    Ok(key)
}

/// キーチェーンに保存するときのサービス名とユーザー名。
const SERVICE: &str = "io.github.agaemo.nuebako";
const USER: &str = "history-encryption-key";

/// 資格情報を作るときの追加指定。
///
/// Windows の資格情報マネージャーは、既定ではドメイン環境でローミングされる永続性(Enterprise)で保存する。
/// 鍵が別の端末へ複製されうる一方、履歴の DB はこの端末のローカルフォルダにしか無く、保存先と寿命が食い違う。
/// そのため、この端末の中だけに残る `Local` を指定する。macOS の Keychain に該当する指定は無い。
fn credential_modifiers() -> std::collections::HashMap<&'static str, &'static str> {
    #[cfg(windows)]
    return std::collections::HashMap::from([("persistence", "Local")]);
    #[cfg(not(windows))]
    std::collections::HashMap::new()
}

/// OS のキーチェーン(macOS の Keychain、Windows の資格情報マネージャー)に鍵を保存する実装。
/// keyring-core の既定ストアを初期化して `Entry` を使うだけの薄いラッパー。
pub struct KeychainKeyStore {
    service: String,
    user: String,
}

impl Default for KeychainKeyStore {
    fn default() -> Self {
        Self::new(SERVICE, USER)
    }
}

impl KeychainKeyStore {
    pub fn new(service: &str, user: &str) -> Self {
        Self {
            service: service.to_string(),
            user: user.to_string(),
        }
    }

    /// 既定のストアを一度だけ設定する。失敗したら以降の呼び出しもエラーにする。
    fn init_store() -> Result<(), AppError> {
        use std::sync::OnceLock;
        static INIT: OnceLock<Result<(), ()>> = OnceLock::new();
        let r = INIT.get_or_init(|| {
            #[cfg(target_os = "macos")]
            let store = apple_native_keyring_store::keychain::Store::new();
            #[cfg(windows)]
            let store = windows_native_keyring_store::Store::new();
            #[cfg(any(target_os = "macos", windows))]
            {
                match store {
                    Ok(s) => {
                        keyring_core::set_default_store(s);
                        Ok(())
                    }
                    Err(_) => Err(()),
                }
            }
            #[cfg(not(any(target_os = "macos", windows)))]
            Err(())
        });
        r.map_err(|_| AppError::KeyUnavailable)
    }

    fn entry(&self) -> Result<keyring_core::Entry, AppError> {
        Self::init_store()?;
        keyring_core::Entry::new_with_modifiers(&self.service, &self.user, &credential_modifiers())
            .map_err(|_| AppError::KeyUnavailable)
    }

    /// テスト用: 保存した鍵を削除する。
    #[cfg(test)]
    fn delete(&self) {
        if let Ok(e) = self.entry() {
            let _ = e.delete_credential();
        }
    }
}

impl KeyStore for KeychainKeyStore {
    fn load(&self) -> Result<Option<[u8; 32]>, AppError> {
        match self.entry()?.get_secret() {
            Ok(bytes) => <[u8; 32]>::try_from(bytes.as_slice())
                .map(Some)
                .map_err(|_| AppError::KeyUnavailable),
            Err(keyring_core::Error::NoEntry) => Ok(None),
            Err(_) => Err(AppError::KeyUnavailable),
        }
    }

    fn save(&self, key: &[u8; 32]) -> Result<(), AppError> {
        self.entry()?
            .set_secret(key)
            .map_err(|_| AppError::KeyUnavailable)
    }
}

#[cfg(test)]
pub(crate) mod fake {
    use super::*;
    use std::sync::Mutex;

    /// テスト用の偽のキーストア。`fail` が true なら、取得・保存が失敗する。
    #[derive(Default)]
    pub struct FakeKeyStore {
        pub key: Mutex<Option<[u8; 32]>>,
        pub fail: Mutex<bool>,
        pub save_count: Mutex<u32>,
    }

    impl FakeKeyStore {
        pub fn with_key(key: [u8; 32]) -> Self {
            Self {
                key: Mutex::new(Some(key)),
                ..Default::default()
            }
        }
        pub fn failing() -> Self {
            Self {
                fail: Mutex::new(true),
                ..Default::default()
            }
        }
    }

    impl KeyStore for FakeKeyStore {
        fn load(&self) -> Result<Option<[u8; 32]>, AppError> {
            if *self.fail.lock().unwrap() {
                return Err(AppError::KeyUnavailable);
            }
            Ok(*self.key.lock().unwrap())
        }
        fn save(&self, key: &[u8; 32]) -> Result<(), AppError> {
            if *self.fail.lock().unwrap() {
                return Err(AppError::KeyUnavailable);
            }
            *self.key.lock().unwrap() = Some(*key);
            *self.save_count.lock().unwrap() += 1;
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::fake::FakeKeyStore;
    use super::*;

    #[test]
    fn 鍵が無いとき生成して保存する() {
        let store = FakeKeyStore::default();
        let key = load_or_create(&store).unwrap();
        assert_eq!(store.load().unwrap(), Some(key));
        assert_eq!(*store.save_count.lock().unwrap(), 1);
    }

    #[test]
    fn 鍵があるとき再利用して保存しない() {
        let store = FakeKeyStore::with_key([3u8; 32]);
        assert_eq!(load_or_create(&store).unwrap(), [3u8; 32]);
        assert_eq!(*store.save_count.lock().unwrap(), 0);
    }

    #[test]
    fn 取得に失敗したときエラーを返し鍵を作らない() {
        let store = FakeKeyStore::failing();
        assert_eq!(load_or_create(&store), Err(AppError::KeyUnavailable));
        assert_eq!(*store.save_count.lock().unwrap(), 0);
        assert!(store.key.lock().unwrap().is_none());
    }

    #[cfg(windows)]
    #[test]
    fn windowsでは資格情報の永続性をローカルにする() {
        assert_eq!(credential_modifiers().get("persistence"), Some(&"Local"));
    }

    #[cfg(not(windows))]
    #[test]
    fn windows以外では追加の指定をしない() {
        assert!(credential_modifiers().is_empty());
    }

    /// 実機のキーチェーンに触れるため、手動で実行する(`cargo test -- --ignored`)。
    #[test]
    #[ignore]
    fn キーチェーンに保存した鍵を読み出せる() {
        let store = KeychainKeyStore::new("io.github.agaemo.nuebako.test", "test-key");
        store.delete();
        assert_eq!(store.load().unwrap(), None);
        store.save(&[9u8; 32]).unwrap();
        assert_eq!(store.load().unwrap(), Some([9u8; 32]));
        store.delete();
    }
}

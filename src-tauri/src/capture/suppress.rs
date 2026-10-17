//! 自己書き込みの抑止(貼り付けで書き込んだ内容を、履歴に記録しない)。track-clipboard が実装する。
//!
//! 書き込んだ内容のハッシュを覚え、2秒以内に1回だけ同じ内容の変更を無視する。
//! 時刻は呼び出し側が渡す(テストで固定するため)。

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::sync::{Arc, Mutex, OnceLock};

use crate::domain::clip::ClipContent;

/// 抑止の有効時間(ミリ秒)。
pub const SUPPRESS_WINDOW_MS: i64 = 2_000;

/// 内容のハッシュ(履歴の内容そのものは保持しない)。
pub fn hash_content(content: &ClipContent) -> u64 {
    let mut hasher = DefaultHasher::new();
    match content {
        ClipContent::Text(t) => {
            0u8.hash(&mut hasher);
            t.hash(&mut hasher);
        }
        ClipContent::Image { png, .. } => {
            1u8.hash(&mut hasher);
            png.hash(&mut hasher);
        }
    }
    hasher.finish()
}

pub struct Suppressor {
    entry: Mutex<Option<(u64, i64)>>,
}

impl Suppressor {
    pub fn new() -> Self {
        Self {
            entry: Mutex::new(None),
        }
    }

    /// 書き込む内容のハッシュを登録する(前の登録は置き換える)。
    pub fn register(&self, hash: u64, now_ms: i64) {
        if let Ok(mut e) = self.entry.lock() {
            *e = Some((hash, now_ms));
        }
    }

    /// 登録を取り消す(書き込みに失敗したとき)。
    pub fn clear(&self) {
        if let Ok(mut e) = self.entry.lock() {
            *e = None;
        }
    }

    /// 同じハッシュの変更を無視すべきか。無視するときは登録を消費する(1回だけ)。
    pub fn should_ignore(&self, hash: u64, now_ms: i64) -> bool {
        let Ok(mut e) = self.entry.lock() else {
            return false;
        };
        match *e {
            Some((_, at)) if now_ms - at > SUPPRESS_WINDOW_MS => {
                // 期限切れの登録は捨てる
                *e = None;
                false
            }
            Some((h, _)) if h == hash => {
                *e = None;
                true
            }
            _ => false,
        }
    }
}

impl Default for Suppressor {
    fn default() -> Self {
        Self::new()
    }
}

/// 監視と貼り付けで共有する抑止。
pub fn shared() -> Arc<Suppressor> {
    static SHARED: OnceLock<Arc<Suppressor>> = OnceLock::new();
    SHARED.get_or_init(|| Arc::new(Suppressor::new())).clone()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn text(s: &str) -> ClipContent {
        ClipContent::Text(s.to_string())
    }

    #[test]
    fn 同じ内容のハッシュは等しく違う内容は異なる() {
        assert_eq!(hash_content(&text("a")), hash_content(&text("a")));
        assert_ne!(hash_content(&text("a")), hash_content(&text("b")));
    }

    #[test]
    fn 登録した内容を2秒以内に1回だけ無視する() {
        let s = Suppressor::new();
        let h = hash_content(&text("x"));
        s.register(h, 1_000);
        assert!(s.should_ignore(h, 1_500));
        assert!(!s.should_ignore(h, 1_600), "2回目は記録される");
    }

    #[test]
    fn 登録から2秒ちょうどまでは無視する() {
        let s = Suppressor::new();
        let h = hash_content(&text("x"));
        s.register(h, 1_000);
        assert!(s.should_ignore(h, 3_000));
    }

    #[test]
    fn 登録から2秒を超えたら無視しない() {
        let s = Suppressor::new();
        let h = hash_content(&text("x"));
        s.register(h, 1_000);
        assert!(!s.should_ignore(h, 3_001));
    }

    #[test]
    fn 違う内容の変更は無視せず登録も残る() {
        let s = Suppressor::new();
        let h = hash_content(&text("x"));
        s.register(h, 1_000);
        assert!(!s.should_ignore(hash_content(&text("y")), 1_100));
        assert!(s.should_ignore(h, 1_200));
    }

    #[test]
    fn 登録がなければ無視しない() {
        let s = Suppressor::new();
        assert!(!s.should_ignore(hash_content(&text("x")), 1_000));
    }

    #[test]
    fn 無視した後に利用者が同じ内容を改めてコピーすると記録される() {
        let s = Suppressor::new();
        let h = hash_content(&text("x"));
        s.register(h, 1_000);
        assert!(s.should_ignore(h, 1_100));
        assert!(!s.should_ignore(h, 1_200));
    }

    #[test]
    fn 取り消すと無視しない() {
        let s = Suppressor::new();
        let h = hash_content(&text("x"));
        s.register(h, 1_000);
        s.clear();
        assert!(!s.should_ignore(h, 1_100));
    }

    #[test]
    fn 新しい登録は前の登録を置き換える() {
        let s = Suppressor::new();
        let a = hash_content(&text("a"));
        let b = hash_content(&text("b"));
        s.register(a, 1_000);
        s.register(b, 1_100);
        assert!(!s.should_ignore(a, 1_200));
        assert!(s.should_ignore(b, 1_200));
    }
}

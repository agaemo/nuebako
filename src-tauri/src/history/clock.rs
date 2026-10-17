//! 時計。テストでは固定の時計を注入する。

use std::time::{SystemTime, UNIX_EPOCH};

pub trait Clock: Send + Sync {
    /// 現在時刻(UTC の Unix ミリ秒)
    fn now_ms(&self) -> i64;
}

pub struct SystemClock;

impl Clock for SystemClock {
    fn now_ms(&self) -> i64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_millis() as i64)
            .unwrap_or(0)
    }
}

/// テスト用の固定時計。`set`・`advance` で時刻を動かせる。
#[cfg(test)]
pub(crate) struct FixedClock(std::sync::atomic::AtomicI64);

#[cfg(test)]
impl FixedClock {
    pub fn new(ms: i64) -> std::sync::Arc<Self> {
        std::sync::Arc::new(Self(std::sync::atomic::AtomicI64::new(ms)))
    }
    pub fn set(&self, ms: i64) {
        self.0.store(ms, std::sync::atomic::Ordering::SeqCst);
    }
    pub fn advance(&self, ms: i64) {
        self.0.fetch_add(ms, std::sync::atomic::Ordering::SeqCst);
    }
}

#[cfg(test)]
impl Clock for FixedClock {
    fn now_ms(&self) -> i64 {
        self.0.load(std::sync::atomic::Ordering::SeqCst)
    }
}

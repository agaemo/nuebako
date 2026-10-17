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

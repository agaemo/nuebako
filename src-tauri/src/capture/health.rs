//! 記録が止まっていないかの判定。取り込みが続けて失敗するのに、利用者に何も見えないままだと、
//! 履歴が増えないことに気づけないため、画面に「記録停止」として知らせる。

use std::sync::atomic::{AtomicU32, Ordering};

use super::source::{CaptureResult, WatchStatus};
use crate::domain::dto::CaptureStatus;

/// この回数だけ続けて読み取り・記録に失敗したら、記録が止まっているとみなす。
/// 1〜2回は、他のアプリがクリップボードを開いている間の一時的な失敗でも起きるため。
/// ファイルのコピーなど、テキストでも画像でもない内容のコピーも「読めない」に数えるので、
/// 偶然の連続で誤って知らせないよう、余裕を持たせる。
pub const CONSECUTIVE_FAILURE_LIMIT: u32 = 10;

/// 取り込みの失敗が続いた回数。成功した(または意図して記録しなかった)ら、数え直す。
#[derive(Default)]
pub struct CaptureHealth {
    failures: AtomicU32,
}

impl CaptureHealth {
    /// 1回分の取り込みの結果を数える。
    pub fn observe(&self, result: &CaptureResult) {
        match result {
            CaptureResult::Unreadable | CaptureResult::Failed(_) => {
                self.failures.fetch_add(1, Ordering::SeqCst);
            }
            _ => self.failures.store(0, Ordering::SeqCst),
        }
    }

    /// 失敗が上限まで続いているか。
    pub fn is_failing(&self) -> bool {
        self.failures.load(Ordering::SeqCst) >= CONSECUTIVE_FAILURE_LIMIT
    }
}

/// 監視の状態と失敗の連続から、画面に出す記録の状態を決める。
pub fn capture_status(watch: WatchStatus, health: &CaptureHealth) -> CaptureStatus {
    if watch == WatchStatus::GaveUp || health.is_failing() {
        CaptureStatus::Stopped
    } else {
        CaptureStatus::Ok
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::history::{IgnoreReason, RecordOutcome};

    fn failures(h: &CaptureHealth, n: u32, r: CaptureResult) {
        for _ in 0..n {
            h.observe(&match &r {
                CaptureResult::Unreadable => CaptureResult::Unreadable,
                _ => CaptureResult::Failed("clipboard".into()),
            });
        }
    }

    #[test]
    fn 監視が再起動の上限を超えたとき停止と判定する() {
        let h = CaptureHealth::default();
        assert_eq!(
            capture_status(WatchStatus::GaveUp, &h),
            CaptureStatus::Stopped
        );
    }

    #[test]
    fn 監視が動いていて失敗も続いていないとき正常と判定する() {
        let h = CaptureHealth::default();
        for w in [
            WatchStatus::NotStarted,
            WatchStatus::Running,
            WatchStatus::Stopped,
        ] {
            assert_eq!(capture_status(w, &h), CaptureStatus::Ok);
        }
    }

    #[test]
    fn 読めない内容が上限まで続いたとき停止と判定する() {
        let h = CaptureHealth::default();
        failures(&h, CONSECUTIVE_FAILURE_LIMIT - 1, CaptureResult::Unreadable);
        assert_eq!(capture_status(WatchStatus::Running, &h), CaptureStatus::Ok);
        h.observe(&CaptureResult::Unreadable);
        assert_eq!(
            capture_status(WatchStatus::Running, &h),
            CaptureStatus::Stopped
        );
    }

    #[test]
    fn 取り込みの失敗が上限まで続いたとき停止と判定する() {
        let h = CaptureHealth::default();
        failures(
            &h,
            CONSECUTIVE_FAILURE_LIMIT,
            CaptureResult::Failed(String::new()),
        );
        assert!(h.is_failing());
    }

    #[test]
    fn 途中で記録できたら数え直し_回復したら正常に戻る() {
        let h = CaptureHealth::default();
        failures(&h, CONSECUTIVE_FAILURE_LIMIT, CaptureResult::Unreadable);
        assert!(h.is_failing());
        h.observe(&CaptureResult::Recorded(RecordOutcome::Added(1)));
        assert!(!h.is_failing());
        failures(&h, CONSECUTIVE_FAILURE_LIMIT - 1, CaptureResult::Unreadable);
        h.observe(&CaptureResult::Recorded(RecordOutcome::Ignored(
            IgnoreReason::Blank,
        )));
        failures(&h, 1, CaptureResult::Unreadable);
        assert!(!h.is_failing(), "間に成功が挟まれば連続ではない");
    }

    #[test]
    fn 意図して記録しなかった結果も失敗に数えない() {
        let h = CaptureHealth::default();
        failures(&h, CONSECUTIVE_FAILURE_LIMIT, CaptureResult::Unreadable);
        h.observe(&CaptureResult::Suppressed);
        assert!(!h.is_failing());
    }
}

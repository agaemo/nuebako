//! 貼り付けの手順: 抑止の登録、書き込み、ウィンドウを閉じる(フォーカス復帰を含む)、待機、キー送信、繰り上げ。
//!
//! 実際の OS 操作はすべて trait の裏に置き、この手順だけを偽実装でテストする。

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use super::keys::KeySender;
use super::permission::PastePermission;
use super::writer::ClipboardWriter;
use crate::domain::clip::ClipContent;
use crate::domain::dto::{PasteMode, PasteOutcome, PasteReason, PasteResult};
use crate::error::AppError;
use crate::history::ClipProvider;

/// ウィンドウを閉じてからキーを送るまでの待機(ミリ秒。ADR-003)。
pub const PASTE_WAIT_MS: u64 = 100;

/// 自己書き込みの抑止の登録口。
pub trait WriteSuppressor: Send + Sync {
    /// これから書き込む内容を登録する。
    fn register(&self, content: &ClipContent);
    /// 書き込みに失敗したときに登録を取り消す。
    fn cancel(&self);
}

/// 履歴ウィンドウを閉じ、元のアプリへフォーカスを戻す。
pub trait PasteWindow: Send + Sync {
    fn close_and_restore_focus(&self);
}

/// 待機。テストでは時間を使わない。
pub trait Waiter: Send + Sync {
    fn wait_ms(&self, ms: u64);
}

/// 破棄時に実行中フラグを下ろす。
struct BusyGuard<'a>(&'a AtomicBool);

impl Drop for BusyGuard<'_> {
    fn drop(&mut self) {
        self.0.store(false, Ordering::SeqCst);
    }
}

pub struct PasteFlow {
    pub provider: Arc<dyn ClipProvider>,
    pub writer: Arc<dyn ClipboardWriter>,
    pub keys: Arc<dyn KeySender>,
    pub permission: Arc<dyn PastePermission>,
    pub window: Arc<dyn PasteWindow>,
    pub suppressor: Arc<dyn WriteSuppressor>,
    pub waiter: Arc<dyn Waiter>,
    /// 実行中フラグ(二重実行の防止。複数の `PasteFlow` で共有する)
    pub busy: Arc<AtomicBool>,
}

impl PasteFlow {
    /// 貼り付けを実行する。実行中に呼ばれたら、何もせず `Internal` を返す。
    pub fn run(&self, id: i64, mode: PasteMode) -> Result<PasteOutcome, AppError> {
        if self.busy.swap(true, Ordering::SeqCst) {
            return Err(AppError::Internal("貼り付けを実行中です".to_string()));
        }
        // 終了時(エラー・パニックを含む)に必ず実行中フラグを戻す
        let _guard = BusyGuard(&self.busy);
        self.run_inner(id, mode)
    }

    fn run_inner(&self, id: i64, mode: PasteMode) -> Result<PasteOutcome, AppError> {
        let content = self.provider.get_content(id)?;
        // 書き込みの通知が登録より先に届く競合(Windows はイベント駆動)を避けるため、書き込みの前に登録する
        self.suppressor.register(&content);
        if let Err(e) = self.writer.write(&content) {
            self.suppressor.cancel();
            return Err(e);
        }
        // 閉じる処理は元のアプリへのフォーカス復帰を含む
        self.window.close_and_restore_focus();

        let outcome = if mode == PasteMode::CopyOnly {
            PasteOutcome {
                result: PasteResult::CopiedOnly,
                reason: None,
            }
        } else if !self.permission.is_granted() {
            PasteOutcome {
                result: PasteResult::CopiedOnly,
                reason: Some(PasteReason::NoPermission),
            }
        } else {
            self.waiter.wait_ms(PASTE_WAIT_MS);
            match self.keys.send_paste() {
                Ok(()) => PasteOutcome {
                    result: PasteResult::Pasted,
                    reason: None,
                },
                Err(_) => PasteOutcome {
                    result: PasteResult::CopiedOnly,
                    reason: Some(PasteReason::SendFailed),
                },
            }
        };
        // 繰り上げの失敗は、貼り付けの結果には影響させない
        let _ = self.provider.promote(id);
        Ok(outcome)
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use super::*;

    type Log = Arc<Mutex<Vec<String>>>;

    struct Fake {
        log: Log,
        granted: bool,
        write_fails: bool,
        send_fails: bool,
        get_fails: bool,
    }

    impl ClipProvider for Fake {
        fn get_content(&self, id: i64) -> Result<ClipContent, AppError> {
            self.log.lock().unwrap().push(format!("get:{id}"));
            if self.get_fails {
                return Err(AppError::NotFound);
            }
            Ok(ClipContent::Text("ダミー".to_string()))
        }
        fn promote(&self, id: i64) -> Result<(), AppError> {
            self.log.lock().unwrap().push(format!("promote:{id}"));
            Ok(())
        }
    }
    impl ClipboardWriter for Fake {
        fn write(&self, _content: &ClipContent) -> Result<(), AppError> {
            self.log.lock().unwrap().push("write".into());
            if self.write_fails {
                Err(AppError::Clipboard("失敗".into()))
            } else {
                Ok(())
            }
        }
    }
    impl KeySender for Fake {
        fn send_paste(&self) -> Result<(), AppError> {
            self.log.lock().unwrap().push("send".into());
            if self.send_fails {
                Err(AppError::Internal("失敗".into()))
            } else {
                Ok(())
            }
        }
    }
    impl PastePermission for Fake {
        fn is_granted(&self) -> bool {
            self.granted
        }
    }
    impl PasteWindow for Fake {
        fn close_and_restore_focus(&self) {
            self.log.lock().unwrap().push("close".into());
        }
    }
    impl WriteSuppressor for Fake {
        fn register(&self, _content: &ClipContent) {
            self.log.lock().unwrap().push("suppress".into());
        }
        fn cancel(&self) {
            self.log.lock().unwrap().push("cancel".into());
        }
    }
    impl Waiter for Fake {
        fn wait_ms(&self, ms: u64) {
            self.log.lock().unwrap().push(format!("wait:{ms}"));
        }
    }

    fn flow_with(
        granted: bool,
        write_fails: bool,
        send_fails: bool,
        get_fails: bool,
    ) -> (PasteFlow, Log) {
        let log: Log = Arc::new(Mutex::new(Vec::new()));
        let fake = Arc::new(Fake {
            log: log.clone(),
            granted,
            write_fails,
            send_fails,
            get_fails,
        });
        let flow = PasteFlow {
            provider: fake.clone(),
            writer: fake.clone(),
            keys: fake.clone(),
            permission: fake.clone(),
            window: fake.clone(),
            suppressor: fake.clone(),
            waiter: fake,
            busy: Arc::new(AtomicBool::new(false)),
        };
        (flow, log)
    }

    fn entries(log: &Log) -> Vec<String> {
        log.lock().unwrap().clone()
    }

    #[test]
    fn 自動貼り付けのとき決められた順序で実行する() {
        let (flow, log) = flow_with(true, false, false, false);
        let out = flow.run(7, PasteMode::Auto).unwrap();
        assert_eq!(out.result, PasteResult::Pasted);
        assert_eq!(out.reason, None);
        assert_eq!(
            entries(&log),
            [
                "get:7",
                "suppress",
                "write",
                "close",
                "wait:100",
                "send",
                "promote:7"
            ]
        );
    }

    #[test]
    fn 権限がないとき再コピーのみで理由は権限なしでキーを送らない() {
        let (flow, log) = flow_with(false, false, false, false);
        let out = flow.run(1, PasteMode::Auto).unwrap();
        assert_eq!(out.result, PasteResult::CopiedOnly);
        assert_eq!(out.reason, Some(PasteReason::NoPermission));
        assert!(!entries(&log).contains(&"send".to_string()));
        assert!(entries(&log).contains(&"close".to_string()));
    }

    #[test]
    fn copy_onlyのときキーを送らず理由はなし() {
        let (flow, log) = flow_with(true, false, false, false);
        let out = flow.run(1, PasteMode::CopyOnly).unwrap();
        assert_eq!(out.result, PasteResult::CopiedOnly);
        assert_eq!(out.reason, None);
        let e = entries(&log);
        assert!(!e.contains(&"send".to_string()));
        assert!(e.contains(&"write".to_string()));
        assert!(e.contains(&"close".to_string()));
    }

    #[test]
    fn キー送信に失敗したとき再コピーのみで理由は送信失敗() {
        let (flow, _log) = flow_with(true, false, true, false);
        let out = flow.run(1, PasteMode::Auto).unwrap();
        assert_eq!(out.result, PasteResult::CopiedOnly);
        assert_eq!(out.reason, Some(PasteReason::SendFailed));
    }

    #[test]
    fn 成功したときpromoteを呼ぶ() {
        let (flow, log) = flow_with(true, false, false, false);
        flow.run(42, PasteMode::Auto).unwrap();
        assert!(entries(&log).contains(&"promote:42".to_string()));
    }

    #[test]
    fn 書き込みに失敗したときエラーを返し抑止を取り消しキーも送らない() {
        let (flow, log) = flow_with(true, true, false, false);
        let err = flow.run(1, PasteMode::Auto).unwrap_err();
        assert_eq!(err.code(), "clipboard");
        let e = entries(&log);
        assert!(e.contains(&"cancel".to_string()));
        assert!(!e.contains(&"send".to_string()));
        assert!(!e.contains(&"close".to_string()));
        assert!(!e.iter().any(|s| s.starts_with("promote")));
    }

    #[test]
    fn 履歴が見つからないときエラーを返し何も書かない() {
        let (flow, log) = flow_with(true, false, false, true);
        let err = flow.run(9, PasteMode::Auto).unwrap_err();
        assert_eq!(err.code(), "not_found");
        assert_eq!(entries(&log), ["get:9"]);
    }

    #[test]
    fn 実行中の二重呼び出しは2回目を無視する() {
        let (flow, log) = flow_with(true, false, false, false);
        flow.busy.store(true, Ordering::SeqCst);
        let err = flow.run(1, PasteMode::Auto).unwrap_err();
        assert_eq!(err.code(), "internal");
        assert!(entries(&log).is_empty());
        assert!(flow.busy.load(Ordering::SeqCst), "実行中フラグは他方のもの");
    }

    #[test]
    fn 完了後は再び実行できる() {
        let (flow, _log) = flow_with(true, false, false, false);
        flow.run(1, PasteMode::Auto).unwrap();
        assert!(!flow.busy.load(Ordering::SeqCst));
        assert!(flow.run(1, PasteMode::Auto).is_ok());
    }

    #[test]
    fn 失敗で終わったときも実行中フラグを戻す() {
        let (flow, _log) = flow_with(true, true, false, false);
        let _ = flow.run(1, PasteMode::Auto);
        assert!(!flow.busy.load(Ordering::SeqCst));
    }
}

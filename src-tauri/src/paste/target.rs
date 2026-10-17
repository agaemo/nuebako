//! 貼り付け先の確認。履歴ウィンドウを閉じてからキーを送るまでの間に、利用者が別のアプリへ移っていたり、
//! フォーカスが戻っていなかったりすると、キー(Cmd/Ctrl+V)が意図しないアプリに届き、
//! 履歴の内容が誤って貼り付けられる。送る前に確かめ、確認できなければ送らない。

use super::flow::Waiter;

/// 修飾キー(Alt・Shift・Ctrl)が離されるまで待つ上限(ミリ秒)。
#[cfg(windows)]
const MODIFIER_WAIT_MS: u64 = 500;
/// 修飾キーの状態を確かめる間隔(ミリ秒)。
#[cfg(windows)]
const MODIFIER_POLL_MS: u64 = 10;

/// 貼り付け先の確認と、キー送信前の準備。
pub trait PasteTarget: Send + Sync {
    /// 貼り付け先のアプリが前面にあり、キーを送ってよいか。確認できなければ `false`。
    fn destination_confirmed(&self) -> bool;
    /// 利用者が押している修飾キーが離れるまで待つ(上限あり)。
    /// 押されたままだと、送る Ctrl+V に Alt や Shift が混ざり、貼り付けにならないため。
    fn wait_modifiers_released(&self);
}

/// `is_down` が偽になる(= 離れる)まで、`step_ms` ごとに確かめる。
/// 離れたら `true`。`max_ms` 待っても離れなければ `false`(呼び出し側は、それでも送る)。
#[cfg(any(windows, test))]
pub fn wait_until_released(
    is_down: &dyn Fn() -> bool,
    waiter: &dyn Waiter,
    max_ms: u64,
    step_ms: u64,
) -> bool {
    let mut waited = 0;
    loop {
        if !is_down() {
            return true;
        }
        if waited >= max_ms {
            return false;
        }
        waiter.wait_ms(step_ms);
        waited += step_ms;
    }
}

/// 最前面のアプリが、自アプリではなく識別できるときだけ `true`(macOS)。
/// 履歴ウィンドウを隠したあとも自アプリが最前面のままなら、フォーカスが元のアプリに戻っていない。
#[cfg(any(target_os = "macos", test))]
pub fn foreground_is_other_app(frontmost_id: Option<&str>, own_id: &str) -> bool {
    frontmost_id.is_some_and(|id| !id.is_empty() && !id.eq_ignore_ascii_case(own_id))
}

/// 記録した宛先のウィンドウが、いまも最前面か(Windows)。未記録(0)は確認できないので `false`。
#[cfg(any(windows, test))]
pub fn same_window(recorded: isize, foreground: isize) -> bool {
    recorded != 0 && recorded == foreground
}

/// 履歴ウィンドウを開く前の最前面を、貼り付け先として記録してよいか(Windows)。
/// 自アプリのウィンドウは貼り付け先ではない。タスクバーは、フォーカスを戻しても
/// 文字を受け取れないため、貼り付け先にしない。
#[cfg(any(windows, test))]
pub fn is_recordable_destination(hwnd: isize, is_own_window: bool, class_name: &str) -> bool {
    hwnd != 0 && !is_own_window && class_name != "Shell_TrayWnd"
}

/// 実際の OS に問い合わせる実装。
pub struct SystemPasteTarget {
    waiter: std::sync::Arc<dyn Waiter>,
}

impl SystemPasteTarget {
    pub fn new(waiter: std::sync::Arc<dyn Waiter>) -> Self {
        Self { waiter }
    }
}

impl PasteTarget for SystemPasteTarget {
    fn destination_confirmed(&self) -> bool {
        #[cfg(target_os = "macos")]
        return foreground_is_other_app(
            crate::platform::macos::frontmost_app()
                .as_ref()
                .map(|a| a.id.as_str()),
            crate::platform::apps::SELF_APP_ID,
        );
        #[cfg(windows)]
        return same_window(
            crate::platform::focus::recorded_destination(),
            crate::platform::focus::current_foreground(),
        );
        #[cfg(not(any(target_os = "macos", windows)))]
        true
    }

    fn wait_modifiers_released(&self) {
        #[cfg(windows)]
        {
            let released = wait_until_released(
                &crate::platform::focus::any_modifier_down,
                self.waiter.as_ref(),
                MODIFIER_WAIT_MS,
                MODIFIER_POLL_MS,
            );
            if !released {
                eprintln!("修飾キーが離れないまま、貼り付けのキーを送ります");
            }
        }
        #[cfg(not(windows))]
        let _ = &self.waiter;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;
    use std::sync::Mutex;

    /// 待った時間を記録するだけの偽の待機。
    #[derive(Default)]
    struct FakeWaiter(Mutex<Vec<u64>>);
    impl Waiter for FakeWaiter {
        fn wait_ms(&self, ms: u64) {
            self.0.lock().unwrap().push(ms);
        }
    }

    #[test]
    fn 最前面が自アプリ以外の識別できるアプリなら確認できる() {
        assert!(foreground_is_other_app(
            Some("com.apple.TextEdit"),
            "io.github.agaemo.nuebako"
        ));
    }

    #[test]
    fn 最前面が自アプリのとき確認できない() {
        assert!(!foreground_is_other_app(
            Some("io.github.agaemo.nuebako"),
            "io.github.agaemo.nuebako"
        ));
        assert!(!foreground_is_other_app(
            Some("IO.GITHUB.AGAEMO.NUEBAKO"),
            "io.github.agaemo.nuebako"
        ));
    }

    #[test]
    fn 最前面を識別できないとき確認できない() {
        assert!(!foreground_is_other_app(None, "io.github.agaemo.nuebako"));
        assert!(!foreground_is_other_app(
            Some(""),
            "io.github.agaemo.nuebako"
        ));
    }

    #[test]
    fn 記録した宛先と最前面が同じウィンドウなら確認できる() {
        assert!(same_window(0x1234, 0x1234));
    }

    #[test]
    fn 宛先と最前面が違うとき_または宛先を記録していないとき確認できない() {
        assert!(!same_window(0x1234, 0x5678));
        assert!(!same_window(0, 0), "未記録どうしの一致は確認にならない");
        assert!(!same_window(0, 0x5678));
        assert!(!same_window(0x1234, 0));
    }

    #[test]
    fn 自アプリのウィンドウとタスクバーは宛先に記録しない() {
        assert!(is_recordable_destination(0x10, false, "Notepad"));
        assert!(!is_recordable_destination(0x10, true, "Notepad"));
        assert!(!is_recordable_destination(0x10, false, "Shell_TrayWnd"));
        assert!(!is_recordable_destination(0, false, "Notepad"));
    }

    #[test]
    fn 修飾キーが最初から離れていれば待たない() {
        let waiter = FakeWaiter::default();
        assert!(wait_until_released(&|| false, &waiter, 500, 10));
        assert!(waiter.0.lock().unwrap().is_empty());
    }

    #[test]
    fn 修飾キーが途中で離れたらそこで待つのをやめる() {
        let waiter = FakeWaiter::default();
        let polls = Cell::new(0);
        let released = wait_until_released(
            &|| {
                polls.set(polls.get() + 1);
                polls.get() <= 3
            },
            &waiter,
            500,
            10,
        );
        assert!(released);
        assert_eq!(*waiter.0.lock().unwrap(), vec![10, 10, 10]);
    }

    #[test]
    fn 修飾キーが離れないときは上限まで待ってあきらめる() {
        let waiter = FakeWaiter::default();
        assert!(!wait_until_released(&|| true, &waiter, 500, 10));
        let waits = waiter.0.lock().unwrap();
        assert_eq!(waits.iter().sum::<u64>(), 500);
        assert_eq!(waits.len(), 50);
    }
}

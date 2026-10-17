//! クリップボードの監視(clipboard-rs を trait で包む)と、変更時の取り込みの手順。
//!
//! 変更の通知 → 形式の一覧 → 記録可否の判定(filter)→ 読み取り → 種類の判定(classify)→
//! `ClipRecorder::record` の順に進む。履歴の内容・コピー元は、ログに出さない。

use std::collections::{HashMap, VecDeque};
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use clipboard_rs::common::RustImage;
use clipboard_rs::{
    Clipboard, ClipboardContext, ClipboardHandler, ClipboardWatcher, ClipboardWatcherContext,
    ContentFormat, RustImageData, WatcherShutdown,
};

use super::classify::classify_text;
use super::filter::{self, Decision, FORMAT_ZERO_MEANS_EXCLUDE, SkipReason};
use super::suppress::{Suppressor, hash_content};
use crate::domain::clip::{AppRef, ClipContent, ClipKind, NewClip};
use crate::domain::settings::Settings;
use crate::error::AppError;
use crate::history::clock::Clock;
use crate::history::{ClipRecorder, RecordOutcome};
use crate::platform::AppInspector;
use crate::platform::apps::resolve_source;

/// macOS で、コピー元アプリの bundle id を書く慣習の型名。
const SOURCE_HINT_FORMAT: &str = "org.nspasteboard.source";
/// サムネイルの最大の辺(px)。一覧では 18px で表示するため、余裕を持たせる。
const THUMBNAIL_SIZE: u32 = 64;
/// 監視スレッドの再起動の上限(この時間内に、この回数まで)。
const RESTART_WINDOW_MS: i64 = 60_000;
const RESTART_MAX: usize = 5;

/// ある時点のクリップボードの内容。
pub struct ClipboardSnapshot {
    /// 利用できる形式の名前(機密の型名の判定に使う)
    pub formats: Vec<String>,
    /// 値を見て判定する形式(Windows の `CanIncludeInClipboardHistory` など)の値。形式名をキーにする
    pub format_values: HashMap<String, Vec<u8>>,
    pub text: Option<String>,
    pub image_png: Option<Vec<u8>>,
    /// `org.nspasteboard.source` などから分かったコピー元
    pub source_hint: Option<AppRef>,
}

pub trait ClipboardSource: Send + Sync {
    /// 変更の通知を受けて `on_change` を呼ぶ監視を、専用スレッドで始める(呼び出しはブロックしない)。
    fn start(&self, on_change: Box<dyn Fn() + Send + Sync>) -> Result<(), AppError>;
    /// 監視を止める。
    fn stop(&self);
    /// いまの内容を読む。読めない内容(不正な UTF-16 など)は、該当部分を `None` にする。
    fn snapshot(&self) -> Result<ClipboardSnapshot, AppError>;
}

/// 監視の状態。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WatchStatus {
    NotStarted,
    Running,
    /// 呼び出しにより止めた
    Stopped,
    /// 再起動の上限を超えたため、記録を停止した
    GaveUp,
}

/// 再起動の頻度の制限(1分に5回まで)。時刻は呼び出し側が渡す。
pub struct RestartLimiter {
    times: VecDeque<i64>,
}

impl RestartLimiter {
    pub fn new() -> Self {
        Self {
            times: VecDeque::new(),
        }
    }

    /// 再起動してよいか。よい場合は記録する。
    pub fn allow(&mut self, now_ms: i64) -> bool {
        while let Some(front) = self.times.front() {
            if now_ms - *front > RESTART_WINDOW_MS {
                self.times.pop_front();
            } else {
                break;
            }
        }
        if self.times.len() >= RESTART_MAX {
            return false;
        }
        self.times.push_back(now_ms);
        true
    }
}

impl Default for RestartLimiter {
    fn default() -> Self {
        Self::new()
    }
}

/// clipboard-rs の変更通知を、登録された関数に渡すだけのハンドラ。
struct ChangeHandler {
    on_change: Arc<dyn Fn() + Send + Sync>,
}

impl ClipboardHandler for ChangeHandler {
    fn on_clipboard_change(&mut self) {
        (self.on_change)();
    }
}

struct Shared {
    stop: AtomicBool,
    shutdown: Mutex<Option<WatcherShutdown>>,
    status: Mutex<WatchStatus>,
}

/// clipboard-rs による監視。macOS は 500ms ごとの更新回数の確認、Windows はイベント駆動。
pub struct ClipboardRsSource {
    shared: Arc<Shared>,
    clock: Arc<dyn Clock>,
}

impl ClipboardRsSource {
    pub fn new(clock: Arc<dyn Clock>) -> Self {
        Self {
            shared: Arc::new(Shared {
                stop: AtomicBool::new(false),
                shutdown: Mutex::new(None),
                status: Mutex::new(WatchStatus::NotStarted),
            }),
            clock,
        }
    }

    pub fn status(&self) -> WatchStatus {
        self.shared
            .status
            .lock()
            .map(|s| *s)
            .unwrap_or(WatchStatus::GaveUp)
    }
}

fn set_status(shared: &Shared, status: WatchStatus) {
    if let Ok(mut s) = shared.status.lock() {
        *s = status;
    }
}

/// 監視スレッドの本体。パニックは捕まえて再起動する(上限を超えたら記録を停止する)。
fn watch_loop(shared: Arc<Shared>, clock: Arc<dyn Clock>, on_change: Arc<dyn Fn() + Send + Sync>) {
    let mut limiter = RestartLimiter::new();
    let mut first = true;
    loop {
        if shared.stop.load(Ordering::SeqCst) {
            set_status(&shared, WatchStatus::Stopped);
            return;
        }
        if !first && !limiter.allow(clock.now_ms()) {
            set_status(&shared, WatchStatus::GaveUp);
            eprintln!("クリップボードの監視が繰り返し異常終了したため、記録を停止しました");
            return;
        }
        first = false;
        set_status(&shared, WatchStatus::Running);

        let run = catch_unwind(AssertUnwindSafe(|| {
            // Windows では、通知を受けるウィンドウをこのスレッドで作る必要があるため、ここで作る
            let mut ctx = match ClipboardWatcherContext::new() {
                Ok(c) => c,
                Err(_) => return,
            };
            ctx.add_handler(ChangeHandler {
                on_change: on_change.clone(),
            });
            if let Ok(mut slot) = shared.shutdown.lock() {
                *slot = Some(ctx.get_shutdown_channel());
            }
            // 登録の前に stop() が呼ばれていた場合は、すぐ止める
            if shared.stop.load(Ordering::SeqCst)
                && let Ok(mut slot) = shared.shutdown.lock()
            {
                slot.take();
            }
            ctx.start_watch();
        }));
        if run.is_err() {
            eprintln!("クリップボードの監視でパニックが起きたため、再起動します");
        }
        if shared.stop.load(Ordering::SeqCst) {
            set_status(&shared, WatchStatus::Stopped);
            return;
        }
        // 異常終了の直後に、すぐ再開して busy loop にならないよう少し待つ
        std::thread::sleep(Duration::from_millis(500));
    }
}

impl ClipboardSource for ClipboardRsSource {
    fn start(&self, on_change: Box<dyn Fn() + Send + Sync>) -> Result<(), AppError> {
        if self.status() == WatchStatus::Running {
            return Ok(());
        }
        self.shared.stop.store(false, Ordering::SeqCst);
        let shared = self.shared.clone();
        let clock = self.clock.clone();
        let on_change: Arc<dyn Fn() + Send + Sync> = Arc::from(on_change);
        set_status(&shared, WatchStatus::Running);
        std::thread::Builder::new()
            .name("clipboard-watch".to_string())
            .spawn(move || watch_loop(shared, clock, on_change))
            .map_err(|e| AppError::Internal(e.to_string()))?;
        Ok(())
    }

    fn stop(&self) {
        self.shared.stop.store(true, Ordering::SeqCst);
        // 破棄すると停止の合図が送られる
        if let Ok(mut slot) = self.shared.shutdown.lock() {
            slot.take();
        }
    }

    fn snapshot(&self) -> Result<ClipboardSnapshot, AppError> {
        // 変更のたびに作る(ハンドラや構造体に保持しない)
        let ctx = ClipboardContext::new().map_err(|e| AppError::Clipboard(e.to_string()))?;
        let formats = ctx.available_formats().unwrap_or_default();

        let mut format_values = HashMap::new();
        if let Some(name) = formats
            .iter()
            .find(|f| f.eq_ignore_ascii_case(FORMAT_ZERO_MEANS_EXCLUDE))
            && let Ok(v) = ctx.get_buffer(name)
        {
            format_values.insert(name.clone(), v);
        }

        let source_hint = formats
            .iter()
            .any(|f| f == SOURCE_HINT_FORMAT)
            .then(|| ctx.get_buffer(SOURCE_HINT_FORMAT).ok())
            .flatten()
            .and_then(|b| String::from_utf8(b).ok())
            .map(|s| s.trim_matches('\0').trim().to_string())
            .filter(|s| !s.is_empty())
            .map(|id| AppRef {
                name: id.clone(),
                id,
            });

        // 不正な UTF-16 など読めない場合は None にして読み飛ばす
        let text = if ctx.has(ContentFormat::Text) {
            ctx.get_text().ok()
        } else {
            None
        };
        // テキストがあればテキストを優先する(画像の変換は重いので、必要なときだけ読む)
        let text_usable = text.as_deref().is_some_and(|t| !t.trim().is_empty());
        let image_png = if !text_usable && ctx.has(ContentFormat::Image) {
            ctx.get_image()
                .ok()
                .and_then(|img| img.to_png().ok())
                .map(|png| png.get_bytes().to_vec())
        } else {
            None
        };

        Ok(ClipboardSnapshot {
            formats,
            format_values,
            text,
            image_png,
            source_hint,
        })
    }
}

/// PNG から、サムネイル(最大 64px の PNG)を作る。
pub fn make_thumbnail(png: &[u8]) -> Result<Vec<u8>, AppError> {
    let err = |e: Box<dyn std::error::Error + Send + Sync>| AppError::Clipboard(e.to_string());
    let image = RustImageData::from_bytes(png).map_err(err)?;
    let thumb = image
        .thumbnail(THUMBNAIL_SIZE, THUMBNAIL_SIZE)
        .map_err(err)?;
    let out = thumb.to_png().map_err(err)?;
    Ok(out.get_bytes().to_vec())
}

/// 取り込みに必要なものをまとめたもの。
pub struct CaptureDeps {
    pub source: Arc<dyn ClipboardSource>,
    pub inspector: Arc<dyn AppInspector>,
    pub recorder: Arc<dyn ClipRecorder>,
    /// 判定時点の設定を返す(除外アプリ、コピー元不明の扱い)
    pub settings: Arc<dyn Fn() -> Settings + Send + Sync>,
    pub suppressor: Arc<Suppressor>,
    pub clock: Arc<dyn Clock>,
}

/// 1回の変更の処理結果。
#[derive(Debug, PartialEq, Eq)]
pub enum CaptureResult {
    Recorded(RecordOutcome),
    /// 機密形式・除外アプリ・コピー元不明のため記録しなかった
    Skipped(SkipReason),
    /// 自分が書き込んだ内容のため記録しなかった
    Suppressed,
    /// 読める内容がなかった(対応しない形式、不正な UTF-16 など)
    Unreadable,
    Failed(String),
}

/// クリップボードの変更1回分を処理する。
pub fn handle_change(deps: &CaptureDeps) -> CaptureResult {
    let snap = match deps.source.snapshot() {
        Ok(s) => s,
        Err(e) => return CaptureResult::Failed(e.code().to_string()),
    };

    let (content, kind) = match (&snap.text, &snap.image_png) {
        (Some(t), _) if !t.trim().is_empty() => (ClipContent::Text(t.clone()), classify_text(t)),
        (_, Some(png)) => match make_thumbnail(png) {
            Ok(thumb) => (
                ClipContent::Image {
                    png: png.clone(),
                    thumb_png: thumb,
                },
                ClipKind::Image,
            ),
            Err(_) => return CaptureResult::Unreadable,
        },
        // 空白だけのテキストも、記録側(HistoryService)に任せて渡す
        (Some(t), None) => (ClipContent::Text(t.clone()), classify_text(t)),
        (None, None) => return CaptureResult::Unreadable,
    };

    if deps
        .suppressor
        .should_ignore(hash_content(&content), deps.clock.now_ms())
    {
        return CaptureResult::Suppressed;
    }

    let settings = (deps.settings)();
    let source_app = resolve_source(
        snap.source_hint.clone(),
        deps.inspector.clipboard_owner(),
        deps.inspector.frontmost_app(),
    );
    let value_of = |name: &str| -> Option<Vec<u8>> {
        snap.format_values
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case(name))
            .map(|(_, v)| v.clone())
    };
    if let Decision::Skip(reason) = filter::decide(
        &snap.formats,
        &value_of,
        source_app.as_ref(),
        &settings.excluded_apps,
        settings.record_unknown_source,
    ) {
        return CaptureResult::Skipped(reason);
    }

    match deps.recorder.record(NewClip {
        kind,
        content,
        source_app,
    }) {
        Ok(outcome) => CaptureResult::Recorded(outcome),
        Err(e) => CaptureResult::Failed(e.code().to_string()),
    }
}

/// 監視を始める。変更のたびに `handle_change` を呼ぶ(失敗は種別だけを標準エラーに出す)。
pub fn start_capture(deps: Arc<CaptureDeps>) -> Result<(), AppError> {
    let source = deps.source.clone();
    source.start(Box::new(move || {
        if let CaptureResult::Failed(code) = handle_change(&deps) {
            eprintln!("クリップボードの取り込みに失敗しました: {code}");
        }
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::settings::Settings;

    struct FixedClock(i64);
    impl Clock for FixedClock {
        fn now_ms(&self) -> i64 {
            self.0
        }
    }

    struct FakeSource(Mutex<Option<ClipboardSnapshot>>);
    impl ClipboardSource for FakeSource {
        fn start(&self, _on_change: Box<dyn Fn() + Send + Sync>) -> Result<(), AppError> {
            Ok(())
        }
        fn stop(&self) {}
        fn snapshot(&self) -> Result<ClipboardSnapshot, AppError> {
            self.0
                .lock()
                .unwrap()
                .take()
                .ok_or_else(|| AppError::Clipboard("なし".into()))
        }
    }

    struct FakeInspector {
        frontmost: Option<AppRef>,
    }
    impl AppInspector for FakeInspector {
        fn frontmost_app(&self) -> Option<AppRef> {
            self.frontmost.clone()
        }
        fn running_apps(&self) -> Result<Vec<AppRef>, AppError> {
            Ok(vec![])
        }
        fn clipboard_owner(&self) -> Option<AppRef> {
            None
        }
    }

    #[derive(Default)]
    struct FakeRecorder(Mutex<Vec<NewClip>>);
    impl ClipRecorder for FakeRecorder {
        fn record(&self, new: NewClip) -> Result<RecordOutcome, AppError> {
            self.0.lock().unwrap().push(new);
            Ok(RecordOutcome::Added(1))
        }
    }

    fn app(id: &str) -> AppRef {
        AppRef {
            id: id.to_string(),
            name: id.to_string(),
        }
    }

    fn text_snapshot(text: &str, formats: &[&str]) -> ClipboardSnapshot {
        ClipboardSnapshot {
            formats: formats.iter().map(|s| s.to_string()).collect(),
            format_values: HashMap::new(),
            text: Some(text.to_string()),
            image_png: None,
            source_hint: None,
        }
    }

    struct Setup {
        deps: CaptureDeps,
        recorder: Arc<FakeRecorder>,
        suppressor: Arc<Suppressor>,
    }

    fn setup(snap: ClipboardSnapshot, frontmost: Option<AppRef>, settings: Settings) -> Setup {
        let recorder = Arc::new(FakeRecorder::default());
        let suppressor = Arc::new(Suppressor::new());
        let deps = CaptureDeps {
            source: Arc::new(FakeSource(Mutex::new(Some(snap)))),
            inspector: Arc::new(FakeInspector { frontmost }),
            recorder: recorder.clone(),
            settings: Arc::new(move || settings.clone()),
            suppressor: suppressor.clone(),
            clock: Arc::new(FixedClock(10_000)),
        };
        Setup {
            deps,
            recorder,
            suppressor,
        }
    }

    #[test]
    fn テキストをコピーしたとき種類とコピー元つきで記録する() {
        let s = setup(
            text_snapshot("https://example.com", &["public.utf8-plain-text"]),
            Some(app("com.apple.Safari")),
            Settings::default(),
        );
        let r = handle_change(&s.deps);
        assert_eq!(r, CaptureResult::Recorded(RecordOutcome::Added(1)));
        let rec = s.recorder.0.lock().unwrap();
        assert_eq!(rec.len(), 1);
        assert_eq!(rec[0].kind, ClipKind::Url);
        assert_eq!(rec[0].source_app.as_ref().unwrap().id, "com.apple.Safari");
    }

    #[test]
    fn 機密形式を含むとき記録しない() {
        let s = setup(
            text_snapshot("ダミー", &["org.nspasteboard.ConcealedType"]),
            Some(app("com.apple.Safari")),
            Settings::default(),
        );
        assert_eq!(
            handle_change(&s.deps),
            CaptureResult::Skipped(SkipReason::SensitiveFormat)
        );
        assert!(s.recorder.0.lock().unwrap().is_empty());
    }

    #[test]
    fn 除外アプリからのコピーは記録しない() {
        // 既定の除外アプリの識別子は OS ごとに違う(macOS は bundle id、Windows は exe 名)ため、既定値から取る
        let excluded = Settings::default().excluded_apps[0].id.clone();
        let s = setup(
            text_snapshot("ダミー", &["public.utf8-plain-text"]),
            Some(app(&excluded)),
            Settings::default(),
        );
        assert_eq!(
            handle_change(&s.deps),
            CaptureResult::Skipped(SkipReason::ExcludedApp)
        );
    }

    #[test]
    fn コピー元の申告があるときは最前面より優先する() {
        let mut snap = text_snapshot("ダミー", &["public.utf8-plain-text"]);
        // 既定の除外アプリの識別子は OS ごとに違うため、既定値から取る
        snap.source_hint = Some(app(&Settings::default().excluded_apps[1].id));
        let s = setup(snap, Some(app("com.apple.Safari")), Settings::default());
        assert_eq!(
            handle_change(&s.deps),
            CaptureResult::Skipped(SkipReason::ExcludedApp)
        );
    }

    #[test]
    fn 最前面が自分自身でコピー元不明の設定が非記録のとき記録しない() {
        let settings = Settings {
            record_unknown_source: false,
            ..Settings::default()
        };
        let s = setup(
            text_snapshot("ダミー", &["public.utf8-plain-text"]),
            Some(app("io.github.agaemo.nuebako")),
            settings,
        );
        assert_eq!(
            handle_change(&s.deps),
            CaptureResult::Skipped(SkipReason::UnknownSource)
        );
    }

    #[test]
    fn コピー元不明で設定が記録のとき記録する() {
        let s = setup(
            text_snapshot("ダミー", &["public.utf8-plain-text"]),
            None,
            Settings::default(),
        );
        assert_eq!(
            handle_change(&s.deps),
            CaptureResult::Recorded(RecordOutcome::Added(1))
        );
        assert!(s.recorder.0.lock().unwrap()[0].source_app.is_none());
    }

    #[test]
    fn 自分が書き込んだ内容のとき一度だけ記録しない() {
        let s = setup(
            text_snapshot("ダミー", &["public.utf8-plain-text"]),
            Some(app("com.apple.Safari")),
            Settings::default(),
        );
        s.suppressor
            .register(hash_content(&ClipContent::Text("ダミー".into())), 9_500);
        assert_eq!(handle_change(&s.deps), CaptureResult::Suppressed);
        assert!(s.recorder.0.lock().unwrap().is_empty());
    }

    #[test]
    fn Windowsの履歴対象外の値が0のとき記録しない() {
        let mut snap = text_snapshot("ダミー", &["CanIncludeInClipboardHistory"]);
        snap.format_values
            .insert("CanIncludeInClipboardHistory".into(), vec![0, 0, 0, 0]);
        let s = setup(snap, Some(app("a.exe")), Settings::default());
        assert_eq!(
            handle_change(&s.deps),
            CaptureResult::Skipped(SkipReason::SensitiveFormat)
        );
    }

    #[test]
    fn 読める内容がないとき記録しない() {
        let snap = ClipboardSnapshot {
            formats: vec![],
            format_values: HashMap::new(),
            text: None,
            image_png: None,
            source_hint: None,
        };
        let s = setup(snap, None, Settings::default());
        assert_eq!(handle_change(&s.deps), CaptureResult::Unreadable);
    }

    #[test]
    fn 空白だけのテキストは記録側の判定に任せて渡す() {
        let s = setup(
            text_snapshot("   ", &["public.utf8-plain-text"]),
            Some(app("com.apple.Safari")),
            Settings::default(),
        );
        assert_eq!(
            handle_change(&s.deps),
            CaptureResult::Recorded(RecordOutcome::Added(1))
        );
        assert_eq!(s.recorder.0.lock().unwrap().len(), 1);
    }

    #[test]
    fn スナップショットの取得に失敗したときfailedを返す() {
        let s = setup(text_snapshot("ダミー", &[]), None, Settings::default());
        let _ = handle_change(&s.deps); // 1回目で取り出される
        assert!(matches!(handle_change(&s.deps), CaptureResult::Failed(_)));
    }

    #[test]
    fn 再起動は1分に5回まで許可する() {
        let mut l = RestartLimiter::new();
        for i in 0..5 {
            assert!(l.allow(i * 1_000), "{i}回目");
        }
        assert!(!l.allow(5_000), "6回目は拒否");
    }

    #[test]
    fn 記録から1分を過ぎた再起動の記録は数えない() {
        let mut l = RestartLimiter::new();
        for i in 0..5 {
            assert!(l.allow(i * 1_000));
        }
        assert!(!l.allow(59_000));
        assert!(l.allow(61_001), "最初の記録から1分を超えたので許可");
    }

    /// 1x1 の PNG(base64)を復号する。
    fn tiny_png() -> Vec<u8> {
        let b64 = "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNkYPhfDwAChwGA60e6kgAAAABJRU5ErkJggg==";
        let table = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
        let mut out = Vec::new();
        let mut buf = 0u32;
        let mut bits = 0;
        for c in b64.bytes().filter(|c| *c != b'=') {
            buf = (buf << 6) | table.iter().position(|t| *t == c).unwrap() as u32;
            bits += 6;
            if bits >= 8 {
                bits -= 8;
                out.push((buf >> bits) as u8);
                buf &= (1 << bits) - 1;
            }
        }
        out
    }

    #[test]
    fn 画像だけのとき画像として記録する() {
        let snap = ClipboardSnapshot {
            formats: vec!["public.png".into()],
            format_values: HashMap::new(),
            text: None,
            image_png: Some(tiny_png()),
            source_hint: None,
        };
        let s = setup(
            snap,
            Some(app("com.apple.screencaptureui")),
            Settings::default(),
        );
        assert_eq!(
            handle_change(&s.deps),
            CaptureResult::Recorded(RecordOutcome::Added(1))
        );
        let rec = s.recorder.0.lock().unwrap();
        assert_eq!(rec[0].kind, ClipKind::Image);
        assert!(
            matches!(&rec[0].content, ClipContent::Image { thumb_png, .. } if !thumb_png.is_empty())
        );
    }

    #[test]
    fn 画像のデータが壊れているとき記録しない() {
        let snap = ClipboardSnapshot {
            formats: vec!["public.png".into()],
            format_values: HashMap::new(),
            text: None,
            image_png: Some(vec![1, 2, 3]),
            source_hint: None,
        };
        let s = setup(snap, None, Settings::default());
        assert_eq!(handle_change(&s.deps), CaptureResult::Unreadable);
    }

    /// 実機のクリップボードを読む。手元で明示的に実行する。
    #[test]
    #[ignore]
    fn 実機でスナップショットが取れる() {
        let src = ClipboardRsSource::new(Arc::new(crate::history::clock::SystemClock));
        assert!(src.snapshot().is_ok());
    }
}

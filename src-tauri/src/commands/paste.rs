//! 貼り付け。track-clipboard が実装する。
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, LazyLock};
use std::time::Duration;

use tauri::{AppHandle, Manager};

use crate::app_state::AppState;
use crate::capture::suppress::{self, hash_content};
use crate::domain::clip::ClipContent;
use crate::domain::dto::{PasteMode, PasteOutcome};
use crate::error::AppError;
use crate::history::ClipProvider;
use crate::history::clock::{Clock, SystemClock};
use crate::paste::flow::{PasteFlow, PasteWindow, Waiter, WriteSuppressor};
use crate::paste::keys::EnigoKeySender;
use crate::paste::permission::SystemPermission;
use crate::paste::writer::ClipboardRsWriter;

/// 実行中フラグ(二重実行の防止)。アプリ全体で1つ。
static BUSY: LazyLock<Arc<AtomicBool>> = LazyLock::new(|| Arc::new(AtomicBool::new(false)));

/// 履歴ウィンドウを閉じる(元のアプリへのフォーカス復帰を含む)。
struct HistoryWindow(AppHandle);

impl PasteWindow for HistoryWindow {
    fn close_and_restore_focus(&self) {
        crate::window::hide_history(&self.0);
    }
}

/// 共有の抑止(監視側と同じもの)に登録する。
struct SharedSuppressor;

impl WriteSuppressor for SharedSuppressor {
    fn register(&self, content: &ClipContent) {
        suppress::shared().register(hash_content(content), SystemClock.now_ms());
    }

    fn cancel(&self) {
        suppress::shared().clear();
    }
}

struct SleepWaiter;

impl Waiter for SleepWaiter {
    fn wait_ms(&self, ms: u64) {
        std::thread::sleep(Duration::from_millis(ms));
    }
}

/// クリップボードへ設定、ウィンドウを閉じる、フォーカス復帰、(権限があれば)貼り付けを行う。
/// 待機を含むため、メインスレッドを塞がないよう非同期コマンドにして、処理は専用のスレッドで行う。
#[tauri::command]
pub async fn paste_clip(
    app: AppHandle,
    id: i64,
    mode: PasteMode,
) -> Result<PasteOutcome, AppError> {
    let provider: Arc<dyn ClipProvider> = match app.try_state::<AppState>() {
        Some(state) => state.history.clone(),
        None => return Err(AppError::Internal("アプリの状態が未登録です".to_string())),
    };
    let flow = PasteFlow {
        provider,
        writer: Arc::new(ClipboardRsWriter),
        keys: Arc::new(EnigoKeySender::new(app.clone())),
        permission: Arc::new(SystemPermission),
        window: Arc::new(HistoryWindow(app)),
        suppressor: Arc::new(SharedSuppressor),
        waiter: Arc::new(SleepWaiter),
        busy: BUSY.clone(),
    };
    tauri::async_runtime::spawn_blocking(move || flow.run(id, mode))
        .await
        .map_err(|_| AppError::Internal("貼り付けの処理が異常終了しました".to_string()))?
}

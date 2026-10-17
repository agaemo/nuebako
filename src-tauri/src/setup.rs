//! 起動時の配線: 設定、履歴(暗号化と保存)、クリップボードの取り込み、定期処理。

use std::sync::{Arc, Mutex};
use std::time::Duration;

use tauri::{AppHandle, Emitter, Manager};

use crate::app_state::AppState;
use crate::capture::source::{CaptureDeps, ClipboardRsSource, start_capture};
use crate::capture::suppress;
use crate::crypto::keystore::KeychainKeyStore;
use crate::domain::clip::NewClip;
use crate::domain::dto::StorageStatus;
use crate::error::AppError;
use crate::history::clock::{Clock, SystemClock};
use crate::history::{ClipRecorder, HistoryService, RecordOutcome};
use crate::platform::apps::SystemAppInspector;
use crate::settings::store::SettingsStore;
use crate::tray;

/// 履歴の期間・件数の上限を、定期的に適用する間隔。
const LIMITS_INTERVAL: Duration = Duration::from_secs(60 * 60);

/// 監視を止めるために、取り込みの元を保持する。
pub struct CaptureHandle(pub Arc<ClipboardRsSource>);

/// 記録に成功したら、トレイのアイコンを切り替え、画面に履歴の更新を知らせる。
struct NotifyingRecorder {
    inner: Arc<HistoryService>,
    app: AppHandle,
}

impl ClipRecorder for NotifyingRecorder {
    fn record(&self, new: NewClip) -> Result<RecordOutcome, AppError> {
        let outcome = self.inner.record(new)?;
        if matches!(
            outcome,
            RecordOutcome::Added(_) | RecordOutcome::Promoted(_)
        ) {
            tray::flash(&self.app);
            let _ = self.app.emit("history-changed", ());
        }
        Ok(outcome)
    }
}

/// 設定・履歴・取り込みを用意して、アプリの状態として登録する。
pub fn init(app: &AppHandle) -> Result<(), Box<dyn std::error::Error>> {
    let config_dir = app.path().app_config_dir()?;
    let data_dir = app.path().app_data_dir()?;
    std::fs::create_dir_all(&config_dir)?;
    std::fs::create_dir_all(&data_dir)?;

    let settings = Arc::new(SettingsStore::load(config_dir.join("settings.json")));
    let current = settings.get();

    let clock: Arc<dyn Clock> = Arc::new(SystemClock);
    let (history, status, message) = HistoryService::open(
        &data_dir.join("nuebako.db"),
        &KeychainKeyStore::default(),
        clock.clone(),
    );
    let history = Arc::new(history);
    if history.skipped_count() > 0 {
        // 内容は出さず、件数だけを残す
        eprintln!(
            "履歴の読み込みで復号できない行を {} 件読み飛ばしました",
            history.skipped_count()
        );
    }
    history.set_limits(current.max_items, current.retention_days);
    if status == StorageStatus::Ok {
        let _ = history.apply_limits(current.max_items, current.retention_days);
    }

    app.manage(AppState {
        history: history.clone(),
        settings: settings.clone(),
        storage: Mutex::new((status, message)),
    });

    // 鍵や DB が使えないときは、記録を始めない(読み込まず、書き込まず、鍵も作らない)
    if status == StorageStatus::Ok {
        let source = Arc::new(ClipboardRsSource::new(clock.clone()));
        let settings_for_capture = settings.clone();
        let deps = Arc::new(CaptureDeps {
            source: source.clone(),
            inspector: Arc::new(SystemAppInspector),
            recorder: Arc::new(NotifyingRecorder {
                inner: history.clone(),
                app: app.clone(),
            }),
            settings: Arc::new(move || settings_for_capture.get()),
            suppressor: suppress::shared(),
            clock,
        });
        start_capture(deps)?;
        app.manage(CaptureHandle(source));
        spawn_limits_worker(app.clone(), history, settings);
    }
    Ok(())
}

/// 保存期間と件数の上限を、1時間ごとに適用する。
fn spawn_limits_worker(app: AppHandle, history: Arc<HistoryService>, settings: Arc<SettingsStore>) {
    std::thread::spawn(move || {
        loop {
            std::thread::sleep(LIMITS_INTERVAL);
            let s = settings.get();
            if let Ok(removed) = history.apply_limits(s.max_items, s.retention_days)
                && removed > 0
            {
                let _ = app.emit("history-changed", ());
            }
        }
    });
}

/// アプリの終了時に、クリップボードの監視を止める。
pub fn shutdown(app: &AppHandle) {
    if let Some(handle) = app.try_state::<CaptureHandle>() {
        use crate::capture::source::ClipboardSource;
        handle.0.stop();
    }
}

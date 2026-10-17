//! 貼り付けのキー送信(enigo を trait で包む)。macOS は Cmd+V、Windows は Ctrl+V。

use crate::error::AppError;

pub trait KeySender: Send + Sync {
    fn send_paste(&self) -> Result<(), AppError>;
}

/// enigo による貼り付けのキー送信(macOS は Cmd+V、他は Ctrl+V)。
///
/// macOS では、enigo の呼び出しをメインスレッドで行う必要があるため、`run_on_main_thread` の中で実行する。
pub struct EnigoKeySender {
    app: tauri::AppHandle,
}

impl EnigoKeySender {
    pub fn new(app: tauri::AppHandle) -> Self {
        Self { app }
    }
}

/// 修飾キーを押して `v` をクリックし、必ず修飾キーを離す。
fn press_paste() -> Result<(), AppError> {
    use enigo::{Direction, Enigo, Key, Keyboard, Settings};

    #[cfg(target_os = "macos")]
    let modifier = Key::Meta;
    #[cfg(not(target_os = "macos"))]
    let modifier = Key::Control;

    let mut enigo = Enigo::new(&Settings {
        open_prompt_to_get_permissions: false,
        ..Settings::default()
    })
    .map_err(|e| AppError::Internal(e.to_string()))?;
    enigo
        .key(modifier, Direction::Press)
        .map_err(|e| AppError::Internal(e.to_string()))?;
    let clicked = enigo.key(Key::Unicode('v'), Direction::Click);
    let released = enigo.key(modifier, Direction::Release);
    clicked.map_err(|e| AppError::Internal(e.to_string()))?;
    released.map_err(|e| AppError::Internal(e.to_string()))
}

/// メインスレッドで `action` を実行して、結果を待つ。`dispatch` は、実行する関数をメインスレッドのキューに積む処理。
///
/// 時間内に終わらなかったときは、キューに残った関数が後から実行されても何もしないよう印を付けて、失敗を返す。
/// 印が無いと、利用者には「貼り付けに失敗」と伝えたあとで、遅れて Cmd+V が送られ、思わぬ所に貼り付けてしまう。
/// すでに実行が始まっているときは、途中で止められないので、その結果を待つ。
#[cfg(any(target_os = "macos", test))]
fn run_unless_timed_out<R: Send + 'static>(
    dispatch: impl FnOnce(Box<dyn FnOnce() + Send>) -> Result<(), AppError>,
    action: impl FnOnce() -> Result<R, AppError> + Send + 'static,
    timeout: std::time::Duration,
) -> Result<R, AppError> {
    use std::sync::atomic::{AtomicU8, Ordering};
    use std::sync::{Arc, mpsc};

    const PENDING: u8 = 0;
    const RUNNING: u8 = 1;
    const CANCELLED: u8 = 2;
    let state = Arc::new(AtomicU8::new(PENDING));
    let (tx, rx) = mpsc::channel();
    let job_state = state.clone();
    dispatch(Box::new(move || {
        if job_state
            .compare_exchange(PENDING, RUNNING, Ordering::SeqCst, Ordering::SeqCst)
            .is_ok()
        {
            let _ = tx.send(action());
        }
    }))?;
    let timed_out = || AppError::Internal("キー送信が時間内に終わりませんでした".to_string());
    match rx.recv_timeout(timeout) {
        Ok(result) => result,
        Err(_) => {
            match state.compare_exchange(PENDING, CANCELLED, Ordering::SeqCst, Ordering::SeqCst) {
                Ok(_) => Err(timed_out()),
                Err(_) => rx.recv_timeout(timeout).map_err(|_| timed_out())?,
            }
        }
    }
}

impl KeySender for EnigoKeySender {
    fn send_paste(&self) -> Result<(), AppError> {
        #[cfg(target_os = "macos")]
        {
            run_unless_timed_out(
                |job| {
                    self.app
                        .run_on_main_thread(job)
                        .map_err(|e| AppError::Internal(e.to_string()))
                },
                press_paste,
                std::time::Duration::from_secs(3),
            )
        }
        #[cfg(not(target_os = "macos"))]
        {
            let _ = &self.app;
            press_paste()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU32, Ordering};
    use std::sync::{Arc, Mutex};
    use std::time::Duration;

    type Job = Box<dyn FnOnce() + Send>;

    #[test]
    fn 時間内に実行されたら結果を返す() {
        let r = run_unless_timed_out(
            |job| {
                job();
                Ok(())
            },
            || Ok(42),
            Duration::from_millis(100),
        );
        assert_eq!(r, Ok(42));
    }

    #[test]
    fn 時間内に実行されなかったら失敗を返し_遅れて実行されても何もしない() {
        let sent = Arc::new(AtomicU32::new(0));
        let queued: Arc<Mutex<Option<Job>>> = Arc::new(Mutex::new(None));
        let queue = queued.clone();
        let counter = sent.clone();
        let r = run_unless_timed_out(
            move |job| {
                *queue.lock().unwrap() = Some(job);
                Ok(())
            },
            move || {
                counter.fetch_add(1, Ordering::SeqCst);
                Ok(())
            },
            Duration::from_millis(20),
        );
        assert!(r.is_err());
        // メインスレッドが詰まっていて、タイムアウトの後でキューが処理された
        let job = queued.lock().unwrap().take().unwrap();
        job();
        assert_eq!(
            sent.load(Ordering::SeqCst),
            0,
            "遅れて Cmd+V を送ってはいけない"
        );
    }

    #[test]
    fn 実行の最中にタイムアウトしたときは終わるのを待って結果を返す() {
        let r = run_unless_timed_out(
            |job| {
                std::thread::spawn(job);
                Ok(())
            },
            || {
                std::thread::sleep(Duration::from_millis(120));
                Ok("done")
            },
            Duration::from_millis(80),
        );
        assert_eq!(r, Ok("done"));
    }

    #[test]
    fn 積むのに失敗したらそのエラーを返し何も実行しない() {
        let sent = Arc::new(AtomicU32::new(0));
        let counter = sent.clone();
        let r: Result<(), AppError> = run_unless_timed_out(
            |_| Err(AppError::Internal("積めない".into())),
            move || {
                counter.fetch_add(1, Ordering::SeqCst);
                Ok(())
            },
            Duration::from_millis(10),
        );
        assert_eq!(r, Err(AppError::Internal("積めない".into())));
        assert_eq!(sent.load(Ordering::SeqCst), 0);
    }
}

// 骨組み(P1-4)。本体は各トラックで実装し、P3-1 でこの属性を外す。
#![allow(dead_code, unused_variables)]

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

impl KeySender for EnigoKeySender {
    fn send_paste(&self) -> Result<(), AppError> {
        #[cfg(target_os = "macos")]
        {
            let (tx, rx) = std::sync::mpsc::channel();
            self.app
                .run_on_main_thread(move || {
                    let _ = tx.send(press_paste());
                })
                .map_err(|e| AppError::Internal(e.to_string()))?;
            rx.recv_timeout(std::time::Duration::from_secs(3))
                .map_err(|_| {
                    AppError::Internal("キー送信が時間内に終わりませんでした".to_string())
                })?
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
    /// 実際にキーを送る処理は、実機の手動確認(M-4)で行う。ここでは型が組み上がることだけを確認する。
    #[test]
    fn compiled() {}
}

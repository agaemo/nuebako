// テスト名は「条件のとき期待結果」の日本語で書くため、テストでは命名規則の警告を出さない
#![cfg_attr(test, allow(non_snake_case))]

mod app_state;
mod capture;
mod commands;
mod crypto;
mod domain;
mod error;
mod history;
mod paste;
mod platform;
mod settings;
mod storage;
mod tray;
mod window;

use tauri::{Manager, WindowEvent};
use tauri_plugin_global_shortcut::{Code, GlobalShortcutExt, Modifiers, Shortcut, ShortcutState};

/// 履歴ウィンドウを呼び出す既定のショートカット(macOS は Cmd+Shift+V、Windows は Ctrl+Shift+V)。
fn default_shortcut() -> Shortcut {
    #[cfg(target_os = "macos")]
    let modifiers = Modifiers::SUPER | Modifiers::SHIFT;
    #[cfg(not(target_os = "macos"))]
    let modifiers = Modifiers::CONTROL | Modifiers::SHIFT;
    Shortcut::new(Some(modifiers), Code::KeyV)
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        // single-instance は最初に登録する(2つ目の起動では既存の履歴ウィンドウを前面に出す)
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            window::show_history(app);
        }))
        .plugin(
            tauri_plugin_global_shortcut::Builder::new()
                .with_handler(|app, _shortcut, event| {
                    if event.state() == ShortcutState::Pressed {
                        window::toggle_history(app);
                    }
                })
                .build(),
        )
        .on_window_event(|window, event| {
            let label = window.label();
            match event {
                // 履歴ウィンドウは、フォーカスが外れたら隠す
                WindowEvent::Focused(false) if label == window::HISTORY => {
                    window::on_history_blur(window.app_handle());
                }
                // どちらのウィンドウも、閉じる操作は破棄せず隠す(常駐のため)
                WindowEvent::CloseRequested { api, .. }
                    if label == window::HISTORY || label == window::SETTINGS =>
                {
                    api.prevent_close();
                    let _ = window.hide();
                }
                _ => {}
            }
        })
        .setup(|app| {
            let handle = app.handle().clone();
            platform::focus::setup_activation(&handle);
            tray::build(&handle)?;
            handle.global_shortcut().register(default_shortcut())?;
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::app::get_app_status,
            commands::app::hide_window,
            commands::app::open_settings,
            commands::app::open_accessibility_settings,
            commands::history::list_clips,
            commands::history::get_clip_detail,
            commands::history::delete_clip,
            commands::history::clear_history,
            commands::history::set_pinned,
            commands::paste::paste_clip,
            commands::capture::list_running_apps,
            commands::settings::get_settings,
            commands::settings::preview_limits,
            commands::settings::update_settings,
            commands::settings::set_shortcut,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

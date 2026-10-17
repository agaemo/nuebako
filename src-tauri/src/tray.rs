//! トレイアイコンとメニュー(履歴を開く / 設定 / 終了)と、記録したときのアイコンの切り替え。

use std::time::Duration;

use tauri::AppHandle;
use tauri::image::Image;
use tauri::menu::{Menu, MenuItem};
use tauri::tray::TrayIconBuilder;

use crate::window;

const TRAY_ID: &str = "main";

/// 記録した合図として、アイコンを切り替えておく時間。
const FLASH: Duration = Duration::from_millis(600);

// macOS はメニューバーの明暗に合わせるテンプレート画像(黒)、Windows は暗いタスクバー向けの白。
#[cfg(target_os = "macos")]
const NORMAL: &[u8] = include_bytes!("../icons/tray.png");
#[cfg(target_os = "macos")]
const ACTIVE: &[u8] = include_bytes!("../icons/tray-active.png");
#[cfg(not(target_os = "macos"))]
const NORMAL: &[u8] = include_bytes!("../icons/tray-win.png");
#[cfg(not(target_os = "macos"))]
const ACTIVE: &[u8] = include_bytes!("../icons/tray-win-active.png");

/// トレイアイコンを作る。
pub fn build(app: &AppHandle) -> tauri::Result<()> {
    let open = MenuItem::with_id(app, "open_history", "履歴を開く", true, None::<&str>)?;
    let settings = MenuItem::with_id(app, "open_settings", "設定", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "終了", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&open, &settings, &quit])?;

    TrayIconBuilder::with_id(TRAY_ID)
        .icon(Image::from_bytes(NORMAL)?)
        .icon_as_template(true)
        .tooltip("nuebako")
        .menu(&menu)
        .on_menu_event(|app, event| match event.id.as_ref() {
            "open_history" => window::show_history(app),
            "open_settings" => window::show_settings(app),
            "quit" => app.exit(0),
            _ => {}
        })
        .build(app)?;
    Ok(())
}

fn set_icon(app: &AppHandle, bytes: &'static [u8]) {
    let Some(tray) = app.tray_by_id(TRAY_ID) else {
        eprintln!("トレイが見つかりません");
        return;
    };
    match Image::from_bytes(bytes) {
        Ok(image) => {
            // 画像とテンプレートの指定は、1回の呼び出しで同時に反映する(別々だと、反映が揃わないことがある)
            if let Err(e) = tray.set_icon_with_as_template(Some(image), true) {
                eprintln!("トレイのアイコンを変更できません: {e}");
            }
        }
        Err(e) => eprintln!("トレイの画像を読み込めません: {e}"),
    }
}

/// 記録した合図として、アイコンを約 600ms だけ切り替える(通知は出さない)。
pub fn flash(app: &AppHandle) {
    set_icon(app, ACTIVE);
    let app = app.clone();
    std::thread::spawn(move || {
        std::thread::sleep(FLASH);
        set_icon(&app, NORMAL);
    });
}

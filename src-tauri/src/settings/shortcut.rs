//! ショートカットの変更(新規登録に成功してから旧を解除する)。

use std::sync::atomic::{AtomicBool, Ordering};

use tauri::AppHandle;
use tauri_plugin_global_shortcut::{GlobalShortcutExt, Shortcut};

use crate::domain::settings::Settings;
use crate::error::AppError;
use crate::settings::store::SettingsStore;
use crate::settings::validate::validate_accelerator;

/// グローバルショートカットの登録処理(テストでは偽の実装を使う)。
pub trait ShortcutRegistrar: Send + Sync {
    fn register(&self, accelerator: &str) -> Result<(), AppError>;
    fn unregister(&self, accelerator: &str) -> Result<(), AppError>;
}

/// 本物の実装。`tauri_plugin_global_shortcut` への薄いラッパー。
pub struct TauriRegistrar {
    app: AppHandle,
}

impl TauriRegistrar {
    pub fn new(app: AppHandle) -> Self {
        Self { app }
    }
}

impl ShortcutRegistrar for TauriRegistrar {
    fn register(&self, accelerator: &str) -> Result<(), AppError> {
        let shortcut = parse(accelerator)?;
        self.app
            .global_shortcut()
            .register(shortcut)
            .map_err(|e| AppError::ShortcutConflict(e.to_string()))
    }

    fn unregister(&self, accelerator: &str) -> Result<(), AppError> {
        let shortcut = parse(accelerator)?;
        self.app
            .global_shortcut()
            .unregister(shortcut)
            .map_err(|e| AppError::Internal(e.to_string()))
    }
}

/// 文字列を OS 向けのショートカットに変換する。
fn parse(accelerator: &str) -> Result<Shortcut, AppError> {
    accelerator
        .parse::<Shortcut>()
        .map_err(|_| AppError::validation("shortcut", "このキーの組み合わせは使えません"))
}

/// ショートカットを変更する。新規登録に成功してから旧を解除し、保存する。失敗したら元を維持する。
pub fn change_shortcut(
    registrar: &dyn ShortcutRegistrar,
    store: &SettingsStore,
    new: &str,
) -> Result<Settings, AppError> {
    validate_accelerator(new)?;
    let old = store.get().shortcut;
    if old == new {
        return Ok(store.get());
    }
    // 新しいものを先に登録する(失敗したら何も変えない)
    registrar.register(new)?;
    // 旧を解除できないと両方が有効になるため、新を戻して失敗にする
    if let Err(e) = registrar.unregister(&old) {
        let _ = registrar.unregister(new);
        return Err(e);
    }
    match store.save_shortcut(new) {
        Ok(s) => Ok(s),
        Err(e) => {
            // 保存できなかったので、旧に戻す
            let _ = registrar.unregister(new);
            let _ = registrar.register(&old);
            Err(e)
        }
    }
}

/// 起動時に、保存済みのショートカットを登録する。他のアプリと競合して登録できないときは、既定のショートカットを
/// 登録して、設定も既定に更新する。設定を旧のままにすると、画面には登録できなかった値が表示され続け、
/// 実際に有効なショートカットと食い違うため。既定に戻したときは `true` を返す。
pub fn register_at_startup(
    registrar: &dyn ShortcutRegistrar,
    store: &SettingsStore,
) -> Result<bool, AppError> {
    if registrar.register(&store.get().shortcut).is_ok() {
        return Ok(false);
    }
    let default = Settings::default().shortcut;
    registrar.register(&default)?;
    store.save_shortcut(&default)?;
    Ok(true)
}

/// 入力待ちの間だけ、いまのショートカットを一時的に解除する。
/// 押したキーが、グローバルショートカットに奪われず、画面に届くようにするため。
/// 解除したことを `suspended` に記録する(2回呼んでも解除は1回)。
pub fn suspend_shortcut(
    registrar: &dyn ShortcutRegistrar,
    store: &SettingsStore,
    suspended: &AtomicBool,
) -> Result<(), AppError> {
    if suspended.load(Ordering::SeqCst) {
        return Ok(());
    }
    registrar.unregister(&store.get().shortcut)?;
    suspended.store(true, Ordering::SeqCst);
    Ok(())
}

/// 一時解除していたショートカットを、登録し直す。解除していなければ何もしない。
/// 失敗したときは記録を残し、次の機会に再試行できるようにする。
pub fn resume_shortcut(
    registrar: &dyn ShortcutRegistrar,
    store: &SettingsStore,
    suspended: &AtomicBool,
) -> Result<(), AppError> {
    if !suspended.load(Ordering::SeqCst) {
        return Ok(());
    }
    registrar.register(&store.get().shortcut)?;
    suspended.store(false, Ordering::SeqCst);
    Ok(())
}

/// アプリ全体で、ショートカットを一時解除中かどうか。
static SUSPENDED: AtomicBool = AtomicBool::new(false);

/// 本物の登録処理で、入力待ちの間だけ、いまのショートカットを解除する。
pub fn suspend(app: &AppHandle, store: &SettingsStore) -> Result<(), AppError> {
    suspend_shortcut(&TauriRegistrar::new(app.clone()), store, &SUSPENDED)
}

/// 本物の登録処理で、一時解除していたショートカットを戻す。
pub fn resume(app: &AppHandle, store: &SettingsStore) -> Result<(), AppError> {
    resume_shortcut(&TauriRegistrar::new(app.clone()), store, &SUSPENDED)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    /// 呼び出しを記録する偽の登録処理。`fail_register` に含まれる文字列の登録は失敗する。
    #[derive(Default)]
    struct Fake {
        calls: Mutex<Vec<String>>,
        fail_register: Vec<String>,
        fail_unregister: Vec<String>,
    }

    impl ShortcutRegistrar for Fake {
        fn register(&self, a: &str) -> Result<(), AppError> {
            self.calls.lock().unwrap().push(format!("register {a}"));
            if self.fail_register.iter().any(|f| f == a) {
                return Err(AppError::ShortcutConflict("使用中".into()));
            }
            Ok(())
        }
        fn unregister(&self, a: &str) -> Result<(), AppError> {
            self.calls.lock().unwrap().push(format!("unregister {a}"));
            if self.fail_unregister.iter().any(|f| f == a) {
                return Err(AppError::Internal("解除できない".into()));
            }
            Ok(())
        }
    }

    fn new_store(name: &str) -> (SettingsStore, std::path::PathBuf) {
        let dir = std::env::temp_dir().join(format!(
            "nuebako-shortcut-test-{}-{name}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("settings.json");
        (SettingsStore::load(path.clone()), path)
    }

    #[test]
    fn 新規登録に成功したとき旧を解除して保存する() {
        let (store, path) = new_store("ok");
        let old = store.get().shortcut;
        let fake = Fake::default();
        let s = change_shortcut(&fake, &store, "Alt+Shift+K").unwrap();
        assert_eq!(s.shortcut, "Alt+Shift+K");
        assert_eq!(store.get().shortcut, "Alt+Shift+K");
        assert_eq!(
            *fake.calls.lock().unwrap(),
            vec![
                "register Alt+Shift+K".to_string(),
                format!("unregister {old}")
            ]
        );
        assert!(path.exists());
    }

    #[test]
    fn 新規登録に失敗したとき旧を維持して保存せずshortcut_conflict() {
        let (store, path) = new_store("conflict");
        let old = store.get().shortcut;
        let fake = Fake {
            fail_register: vec!["Alt+Shift+K".into()],
            ..Default::default()
        };
        let err = change_shortcut(&fake, &store, "Alt+Shift+K").unwrap_err();
        assert_eq!(err.code(), "shortcut_conflict");
        assert_eq!(store.get().shortcut, old);
        assert_eq!(
            *fake.calls.lock().unwrap(),
            vec!["register Alt+Shift+K".to_string()]
        );
        assert!(!path.exists());
    }

    #[test]
    fn 同じ値のとき何もしない() {
        let (store, path) = new_store("same");
        let old = store.get().shortcut;
        let fake = Fake::default();
        let s = change_shortcut(&fake, &store, &old).unwrap();
        assert_eq!(s.shortcut, old);
        assert!(fake.calls.lock().unwrap().is_empty());
        assert!(!path.exists());
    }

    #[test]
    fn 形式が不正なとき登録を試みない() {
        let (store, _) = new_store("invalid");
        let fake = Fake::default();
        let err = change_shortcut(&fake, &store, "V").unwrap_err();
        assert_eq!(err.code(), "validation");
        assert!(fake.calls.lock().unwrap().is_empty());
    }

    #[test]
    fn 旧の解除に失敗したとき新を戻して元を維持する() {
        let (store, path) = new_store("unreg");
        let old = store.get().shortcut;
        let fake = Fake {
            fail_unregister: vec![old.clone()],
            ..Default::default()
        };
        assert!(change_shortcut(&fake, &store, "Alt+Shift+K").is_err());
        assert_eq!(store.get().shortcut, old);
        assert_eq!(
            *fake.calls.lock().unwrap(),
            vec![
                "register Alt+Shift+K".to_string(),
                format!("unregister {old}"),
                "unregister Alt+Shift+K".to_string()
            ]
        );
        assert!(!path.exists());
    }

    #[test]
    fn 起動時に保存済みのショートカットを登録できたらそのまま使う() {
        let (store, path) = new_store("startup-ok");
        store.save_shortcut("Alt+Shift+K").unwrap();
        let fake = Fake::default();
        assert_eq!(register_at_startup(&fake, &store), Ok(false));
        assert_eq!(
            *fake.calls.lock().unwrap(),
            vec!["register Alt+Shift+K".to_string()]
        );
        assert_eq!(store.get().shortcut, "Alt+Shift+K");
        assert_eq!(SettingsStore::load(path).get().shortcut, "Alt+Shift+K");
    }

    #[test]
    fn 起動時に登録できなかったとき既定を登録して設定も既定に更新する() {
        let (store, path) = new_store("startup-fallback");
        store.save_shortcut("Alt+Shift+K").unwrap();
        let fake = Fake {
            fail_register: vec!["Alt+Shift+K".into()],
            ..Default::default()
        };
        assert_eq!(register_at_startup(&fake, &store), Ok(true));
        let default = Settings::default().shortcut;
        assert_eq!(
            *fake.calls.lock().unwrap(),
            vec![
                "register Alt+Shift+K".to_string(),
                format!("register {default}")
            ]
        );
        assert_eq!(store.get().shortcut, default);
        assert_eq!(
            SettingsStore::load(path).get().shortcut,
            default,
            "ファイルにも保存される"
        );
    }

    #[test]
    fn 既定も登録できないとき設定は変えずにエラーを返す() {
        let (store, _) = new_store("startup-both-fail");
        store.save_shortcut("Alt+Shift+K").unwrap();
        let default = Settings::default().shortcut;
        let fake = Fake {
            fail_register: vec!["Alt+Shift+K".into(), default],
            ..Default::default()
        };
        assert!(register_at_startup(&fake, &store).is_err());
        assert_eq!(store.get().shortcut, "Alt+Shift+K");
    }

    // 入力待ちの間だけ今のショートカットを解除し、終わったら必ず戻す(押したキーを画面が受け取れるようにする)
    use std::sync::atomic::{AtomicBool, Ordering};

    #[test]
    fn 一時解除したとき今のショートカットを解除して印を付ける() {
        let (store, _) = new_store("suspend");
        let current = store.get().shortcut;
        let fake = Fake::default();
        let flag = AtomicBool::new(false);
        suspend_shortcut(&fake, &store, &flag).unwrap();
        assert_eq!(
            *fake.calls.lock().unwrap(),
            vec![format!("unregister {current}")]
        );
        assert!(flag.load(Ordering::SeqCst));
    }

    #[test]
    fn 一時解除を2回呼んでも解除は1回だけ() {
        let (store, _) = new_store("suspend-twice");
        let fake = Fake::default();
        let flag = AtomicBool::new(false);
        suspend_shortcut(&fake, &store, &flag).unwrap();
        suspend_shortcut(&fake, &store, &flag).unwrap();
        assert_eq!(fake.calls.lock().unwrap().len(), 1);
    }

    #[test]
    fn 再開したとき今のショートカットを登録し直して印を外す() {
        let (store, _) = new_store("resume");
        let current = store.get().shortcut;
        let fake = Fake::default();
        let flag = AtomicBool::new(false);
        suspend_shortcut(&fake, &store, &flag).unwrap();
        resume_shortcut(&fake, &store, &flag).unwrap();
        assert_eq!(
            *fake.calls.lock().unwrap(),
            vec![
                format!("unregister {current}"),
                format!("register {current}")
            ]
        );
        assert!(!flag.load(Ordering::SeqCst));
    }

    #[test]
    fn 一時解除していないとき再開しても何もしない() {
        let (store, _) = new_store("resume-noop");
        let fake = Fake::default();
        let flag = AtomicBool::new(false);
        resume_shortcut(&fake, &store, &flag).unwrap();
        assert!(fake.calls.lock().unwrap().is_empty());
    }

    #[test]
    fn 再開に失敗したとき印を残して次の機会に再試行できる() {
        let (store, _) = new_store("resume-fail");
        let current = store.get().shortcut;
        let fake = Fake {
            fail_register: vec![current],
            ..Default::default()
        };
        let flag = AtomicBool::new(false);
        suspend_shortcut(&fake, &store, &flag).unwrap();
        assert!(resume_shortcut(&fake, &store, &flag).is_err());
        assert!(flag.load(Ordering::SeqCst));
    }

    // toAccelerator(src/lib/keys.ts)が作る形式を、Tauri のショートカットとして解釈できること
    #[test]
    fn フロントエンドが作るショートカットの文字列を解釈できる() {
        for accel in [
            "Shift+Command+V",
            "Control+Shift+V",
            "Control+Alt+Shift+Command+K",
            "Alt+1",
            "Control+F12",
            "Control+Up",
            "Alt+Space",
            "Control+/",
            "CommandOrControl+Shift+V",
        ] {
            assert!(parse(accel).is_ok(), "解釈できない: {accel}");
        }
    }
}

//! ショートカットの変更(新規登録に成功してから旧を解除する)。

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
}

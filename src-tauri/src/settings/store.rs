//! `settings.json` の読み書き。

use std::path::PathBuf;
use std::sync::Mutex;

use crate::domain::dto::SettingsPatch;
use crate::domain::settings::Settings;
use crate::error::AppError;
use crate::settings::validate::{dedup_excluded_apps, validate_accelerator, validate_limits};

pub struct SettingsStore {
    path: PathBuf,
    current: Mutex<Settings>,
}

impl SettingsStore {
    /// `path`(settings.json)から読み込む。無ければ既定値。壊れていれば既定値で起動し、壊れたファイルを `.bak` に退避する。
    pub fn load(path: PathBuf) -> Self {
        let settings = match std::fs::read_to_string(&path) {
            Ok(text) => match serde_json::from_str::<Settings>(&text) {
                Ok(s) => s,
                Err(_) => {
                    // 壊れたファイルは消さずに退避する(退避に失敗しても既定値で起動する)
                    let mut bak = path.clone().into_os_string();
                    bak.push(".bak");
                    let _ = std::fs::rename(&path, PathBuf::from(bak));
                    Settings::default()
                }
            },
            Err(_) => Settings::default(),
        };
        Self {
            path,
            current: Mutex::new(settings),
        }
    }

    pub fn get(&self) -> Settings {
        self.current
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
    }

    /// 一時ファイルに書いてから置き換える(書き込み途中で壊れないように)。
    fn write(&self, settings: &Settings) -> Result<(), AppError> {
        let io = |_: std::io::Error| AppError::Storage("設定ファイルを保存できません".to_string());
        let json = serde_json::to_string_pretty(settings)
            .map_err(|_| AppError::Internal("設定を変換できません".to_string()))?;
        if let Some(dir) = self.path.parent() {
            std::fs::create_dir_all(dir).map_err(io)?;
        }
        let mut tmp = self.path.clone().into_os_string();
        tmp.push(".tmp");
        let tmp = PathBuf::from(tmp);
        std::fs::write(&tmp, json).map_err(io)?;
        std::fs::rename(&tmp, &self.path).map_err(io)
    }

    /// 部分更新する。不正な値があれば何も保存しない。
    pub fn update(&self, patch: SettingsPatch) -> Result<Settings, AppError> {
        let mut guard = self.current.lock().unwrap_or_else(|e| e.into_inner());
        let mut next = guard.clone();
        if patch.max_items.is_some() || patch.retention_days.is_some() {
            let max = patch
                .max_items
                .unwrap_or_else(|| next.max_items.to_string());
            let days = patch
                .retention_days
                .unwrap_or_else(|| next.retention_days.to_string());
            (next.max_items, next.retention_days) = validate_limits(&max, &days)?;
        }
        if let Some(theme) = patch.theme {
            next.theme = theme;
        }
        if let Some(apps) = patch.excluded_apps {
            next.excluded_apps = dedup_excluded_apps(apps);
        }
        if let Some(v) = patch.record_unknown_source {
            next.record_unknown_source = v;
        }
        if let Some(v) = patch.permission_guide_dismissed {
            next.permission_guide_dismissed = v;
        }
        self.write(&next)?;
        *guard = next.clone();
        Ok(next)
    }

    /// ショートカットの文字列だけを保存する(登録に成功した後に呼ぶ)。
    pub fn save_shortcut(&self, accelerator: &str) -> Result<Settings, AppError> {
        validate_accelerator(accelerator)?;
        let mut guard = self.current.lock().unwrap_or_else(|e| e.into_inner());
        let mut next = guard.clone();
        next.shortcut = accelerator.to_string();
        self.write(&next)?;
        *guard = next.clone();
        Ok(next)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::clip::AppRef;
    use crate::domain::settings::Theme;
    use std::sync::atomic::{AtomicU32, Ordering};

    static SEQ: AtomicU32 = AtomicU32::new(0);

    /// テストごとに一意な作業ディレクトリを作る。
    fn temp_dir() -> PathBuf {
        let n = SEQ.fetch_add(1, Ordering::SeqCst);
        let dir =
            std::env::temp_dir().join(format!("nuebako-settings-test-{}-{n}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn ファイルが無いとき既定値() {
        let dir = temp_dir();
        let store = SettingsStore::load(dir.join("settings.json"));
        assert_eq!(store.get(), Settings::default());
    }

    #[test]
    fn 保存して読み込み直すと同じ値() {
        let dir = temp_dir();
        let path = dir.join("settings.json");
        let store = SettingsStore::load(path.clone());
        store
            .update(SettingsPatch {
                max_items: Some("500".into()),
                theme: Some(Theme::Dark),
                ..Default::default()
            })
            .unwrap();
        store.save_shortcut("Alt+Shift+K").unwrap();
        let reloaded = SettingsStore::load(path).get();
        assert_eq!(reloaded.max_items, 500);
        assert_eq!(reloaded.theme, Theme::Dark);
        assert_eq!(reloaded.shortcut, "Alt+Shift+K");
    }

    #[test]
    fn 壊れたJSONのとき既定値で起動してbakに退避する() {
        let dir = temp_dir();
        let path = dir.join("settings.json");
        std::fs::write(&path, "{ これはJSONではない").unwrap();
        let store = SettingsStore::load(path.clone());
        assert_eq!(store.get(), Settings::default());
        assert_eq!(
            std::fs::read_to_string(dir.join("settings.json.bak")).unwrap(),
            "{ これはJSONではない"
        );
        assert!(!path.exists());
    }

    #[test]
    fn 不正な更新のとき何も保存しない() {
        let dir = temp_dir();
        let path = dir.join("settings.json");
        let store = SettingsStore::load(path.clone());
        let err = store
            .update(SettingsPatch {
                max_items: Some("0".into()),
                theme: Some(Theme::Dark),
                ..Default::default()
            })
            .unwrap_err();
        assert!(matches!(err, AppError::Validation { ref field, .. } if field == "max_items"));
        assert_eq!(store.get(), Settings::default());
        assert!(!path.exists());
    }

    #[test]
    fn 部分更新のとき指定した項目だけ変わる() {
        let dir = temp_dir();
        let store = SettingsStore::load(dir.join("settings.json"));
        let s = store
            .update(SettingsPatch {
                retention_days: Some("7".into()),
                ..Default::default()
            })
            .unwrap();
        assert_eq!(s.retention_days, 7);
        assert_eq!(s.max_items, Settings::default().max_items);
        assert_eq!(s.excluded_apps, Settings::default().excluded_apps);
    }

    #[test]
    fn 除外アプリの重複はまとめて保存する() {
        let dir = temp_dir();
        let store = SettingsStore::load(dir.join("settings.json"));
        let a = |id: &str| AppRef {
            id: id.into(),
            name: id.into(),
        };
        let s = store
            .update(SettingsPatch {
                excluded_apps: Some(vec![a("x"), a("x"), a("y")]),
                ..Default::default()
            })
            .unwrap();
        assert_eq!(s.excluded_apps, vec![a("x"), a("y")]);
    }

    #[test]
    fn 形式が不正なショートカットは保存しない() {
        let dir = temp_dir();
        let store = SettingsStore::load(dir.join("settings.json"));
        assert!(store.save_shortcut("V").is_err());
        assert_eq!(store.get().shortcut, Settings::default().shortcut);
    }

    #[test]
    fn 一部の項目だけのJSONは残りを既定値で補う() {
        let dir = temp_dir();
        let path = dir.join("settings.json");
        std::fs::write(&path, r#"{"max_items": 42}"#).unwrap();
        let s = SettingsStore::load(path).get();
        assert_eq!(s.max_items, 42);
        assert_eq!(s.retention_days, 30);
    }
}

//! `settings.json` の読み書き。

use std::path::PathBuf;
use std::sync::Mutex;

use crate::domain::dto::SettingsPatch;
use crate::domain::settings::Settings;
use crate::error::AppError;
use crate::settings::validate::{dedup_excluded_apps, validate_accelerator, validate_limits};
use crate::storage::perms;

pub struct SettingsStore {
    path: PathBuf,
    current: Mutex<Settings>,
    /// 設定ファイルを読めず、既定値で起動した(画面に知らせるため)
    recovered: bool,
}

impl SettingsStore {
    /// `path`(settings.json)から読み込む。無ければ既定値(初回の起動なので、異常ではない)。
    /// 壊れていれば既定値で起動し、壊れたファイルを `.bak` に退避する。
    /// 読めなかったときは `recovered` を立てる。除外アプリが黙って既定に戻ったことを、利用者が知れるようにするため。
    pub fn load(path: PathBuf) -> Self {
        let (settings, recovered) = match std::fs::read_to_string(&path) {
            Ok(text) => match serde_json::from_str::<Settings>(&text) {
                Ok(s) => (s, false),
                Err(_) => {
                    // 壊れたファイルは消さずに退避する(退避に失敗しても既定値で起動する)
                    let mut bak = path.clone().into_os_string();
                    bak.push(".bak");
                    let _ = std::fs::rename(&path, PathBuf::from(bak));
                    (Settings::default(), true)
                }
            },
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => (Settings::default(), false),
            Err(_) => (Settings::default(), true),
        };
        Self {
            path,
            current: Mutex::new(settings),
            recovered,
        }
    }

    /// 設定ファイルを読めず、既定値で起動したか。
    pub fn recovered(&self) -> bool {
        self.recovered
    }

    /// 内部の状態を保持したスレッドがパニックしたことがあるか。
    /// 設定は丸ごと置き換える値なので、パニックしても壊れた状態は残らない。履歴と同じく、動作は続けて異常として知らせる。
    pub fn is_poisoned(&self) -> bool {
        self.current.is_poisoned()
    }

    pub fn get(&self) -> Settings {
        self.current
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
    }

    /// 一時ファイルに書いて `sync_all` してから置き換える。書き込み途中や、電源断のあとに、
    /// 空や半端なファイルが残らないようにするため。
    fn write(&self, settings: &Settings) -> Result<(), AppError> {
        use std::io::Write;
        let io = |_: std::io::Error| AppError::Storage("設定ファイルを保存できません".to_string());
        let json = serde_json::to_string_pretty(settings)
            .map_err(|_| AppError::Internal("設定を変換できません".to_string()))?;
        if let Some(dir) = self.path.parent() {
            std::fs::create_dir_all(dir).map_err(io)?;
            perms::restrict_dir(dir);
        }
        let mut tmp = self.path.clone().into_os_string();
        tmp.push(".tmp");
        let tmp = PathBuf::from(tmp);
        let mut options = std::fs::OpenOptions::new();
        options.write(true).create(true).truncate(true);
        #[cfg(unix)]
        {
            // 作成した時点から所有者だけが読めるようにする(あとから絞ると、その間は読める)
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let mut file = options.open(&tmp).map_err(io)?;
        file.write_all(json.as_bytes()).map_err(io)?;
        file.sync_all().map_err(io)?;
        drop(file);
        std::fs::rename(&tmp, &self.path).map_err(io)?;
        perms::restrict_file(&self.path);
        Ok(())
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

    #[test]
    fn 壊れたJSONで起動したときrecoveredが立つ() {
        let dir = temp_dir();
        let path = dir.join("settings.json");
        std::fs::write(&path, "{ これはJSONではない").unwrap();
        assert!(SettingsStore::load(path).recovered());
    }

    #[test]
    fn ファイルが無いとき_または正常に読めたときrecoveredは立たない() {
        let dir = temp_dir();
        let path = dir.join("settings.json");
        assert!(!SettingsStore::load(path.clone()).recovered());
        std::fs::write(&path, r#"{"max_items": 42}"#).unwrap();
        assert!(!SettingsStore::load(path).recovered());
    }

    #[test]
    fn 読み込みに失敗したときもrecoveredが立ち既定値で起動する() {
        // ファイルの場所がフォルダだと、読み込みは NotFound ではない失敗になる
        let dir = temp_dir();
        let path = dir.join("settings.json");
        std::fs::create_dir_all(&path).unwrap();
        let store = SettingsStore::load(path);
        assert!(store.recovered());
        assert_eq!(store.get(), Settings::default());
    }

    #[test]
    fn 保存すると一時ファイルは残らず内容が読み直せる() {
        let dir = temp_dir();
        let path = dir.join("settings.json");
        let store = SettingsStore::load(path.clone());
        store
            .update(SettingsPatch {
                max_items: Some("7".into()),
                ..Default::default()
            })
            .unwrap();
        assert!(!dir.join("settings.json.tmp").exists());
        assert_eq!(SettingsStore::load(path).get().max_items, 7);
    }

    #[cfg(unix)]
    #[test]
    fn 設定ファイルは所有者だけが読み書きできる() {
        use std::os::unix::fs::PermissionsExt;
        let dir = temp_dir();
        let sub = dir.join("conf");
        let path = sub.join("settings.json");
        let store = SettingsStore::load(path.clone());
        store.save_shortcut("Alt+Shift+K").unwrap();
        let mode = |p: &std::path::Path| std::fs::metadata(p).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode(&path), 0o600);
        assert_eq!(mode(&sub), 0o700);
    }

    #[test]
    fn 内部状態を持つスレッドがパニックしても読み書きでき_poisonedを返す() {
        let dir = temp_dir();
        let store = SettingsStore::load(dir.join("settings.json"));
        assert!(!store.is_poisoned());
        let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _guard = store.current.lock().unwrap();
            panic!("テスト用のパニック");
        }));
        assert!(store.is_poisoned());
        assert_eq!(store.get(), Settings::default());
        store
            .update(SettingsPatch {
                theme: Some(Theme::Dark),
                ..Default::default()
            })
            .unwrap();
        assert_eq!(store.get().theme, Theme::Dark);
    }
}

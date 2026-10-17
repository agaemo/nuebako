// 骨組み(P1-4)。本体は各トラックで実装し、P3-1 でこの属性を外す。
#![allow(dead_code, unused_variables)]

//! `settings.json` の読み書き。track-settings が実装する。

use crate::domain::dto::SettingsPatch;
use crate::domain::settings::Settings;
use crate::error::AppError;

pub struct SettingsStore {
    _private: (),
}

impl SettingsStore {
    pub fn get(&self) -> Settings {
        unimplemented!("track-settings")
    }

    /// 部分更新する。不正な値があれば何も保存しない。
    pub fn update(&self, patch: SettingsPatch) -> Result<Settings, AppError> {
        unimplemented!("track-settings")
    }

    /// ショートカットの文字列だけを保存する(登録に成功した後に呼ぶ)。
    pub fn save_shortcut(&self, accelerator: &str) -> Result<Settings, AppError> {
        unimplemented!("track-settings")
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn compiled() {}
}

//! アプリ全体で共有する状態(`tauri::Builder::manage` で登録する)。

use std::sync::Arc;

use crate::domain::dto::StorageStatus;
use crate::history::HistoryService;
use crate::settings::store::SettingsStore;

pub struct AppState {
    pub history: Arc<HistoryService>,
    pub settings: Arc<SettingsStore>,
    /// 起動時の鍵と DB の判定結果と、その理由(起動後は変わらない)
    pub storage: (StorageStatus, Option<String>),
}

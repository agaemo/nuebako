// 骨組み(P1-4)。本体は各トラックで実装し、P3-1 でこの属性を外す。
#![allow(dead_code, unused_variables)]

//! アプリ全体で共有する状態(`tauri::Builder::manage` で登録する。登録は P3-1)。

use std::sync::{Arc, Mutex};

use crate::domain::dto::StorageStatus;
use crate::history::HistoryService;
use crate::settings::store::SettingsStore;

pub struct AppState {
    pub history: Arc<HistoryService>,
    pub settings: Arc<SettingsStore>,
    /// 起動時の鍵と DB の判定結果と、その理由
    pub storage: Mutex<(StorageStatus, Option<String>)>,
}

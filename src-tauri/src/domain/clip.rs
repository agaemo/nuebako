//! 履歴(クリップ)の型。

use serde::{Deserialize, Serialize};

/// 内容の種類。行の左端の色の印に対応する。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ClipKind {
    Text,
    Url,
    Code,
    Path,
    Image,
}

/// アプリの識別情報(macOS は bundle id、Windows は exe 名)と表示名。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AppRef {
    pub id: String,
    pub name: String,
}

/// クリップの内容。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ClipContent {
    Text(String),
    /// 画像(PNG)とサムネイル(PNG)
    Image {
        png: Vec<u8>,
        thumb_png: Vec<u8>,
    },
}

/// 新しく記録するクリップ。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewClip {
    pub kind: ClipKind,
    pub content: ClipContent,
    pub source_app: Option<AppRef>,
}

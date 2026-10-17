//! 設定の保存・検証・ショートカット変更。設定は平文の JSON に保存するため、除外アプリなどの利用者の選択だけを持ち、機密は含めない。

pub mod shortcut;
pub mod store;
pub mod validate;

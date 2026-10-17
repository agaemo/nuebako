// 骨組み(P1-4)。本体は各トラックで実装し、P3-1 でこの属性を外す。
#![allow(dead_code, unused_variables)]

//! ショートカットの変更(新規登録に成功してから旧を解除する)。track-settings が実装する。

use crate::error::AppError;

/// グローバルショートカットの登録処理(テストでは偽の実装を使う)。
pub trait ShortcutRegistrar: Send + Sync {
    fn register(&self, accelerator: &str) -> Result<(), AppError>;
    fn unregister(&self, accelerator: &str) -> Result<(), AppError>;
}

#[cfg(test)]
mod tests {
    #[test]
    fn compiled() {}
}

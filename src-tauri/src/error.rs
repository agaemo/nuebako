//! アプリ共通のエラー。フロントエンドには `{ code, message, field? }` で返す。

use std::fmt;

use serde::ser::{Serialize, SerializeStruct, Serializer};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AppError {
    /// 暗号鍵を取得できない
    KeyUnavailable,
    /// 暗号鍵が保存済みの履歴と合わない
    KeyMismatch,
    /// 保存(SQLite)の失敗。内容は含めない
    Storage(String),
    /// 入力の検証エラー
    Validation {
        field: String,
        message: String,
    },
    /// ショートカットを登録できない(他のアプリと競合など)
    ShortcutConflict(String),
    NotFound,
    /// クリップボードの読み書きの失敗
    Clipboard(String),
    Internal(String),
}

impl AppError {
    /// フロントエンドに返すエラーコード。
    pub fn code(&self) -> &'static str {
        match self {
            AppError::KeyUnavailable => "key_unavailable",
            AppError::KeyMismatch => "key_mismatch",
            AppError::Storage(_) => "storage",
            AppError::Validation { .. } => "validation",
            AppError::ShortcutConflict(_) => "shortcut_conflict",
            AppError::NotFound => "not_found",
            AppError::Clipboard(_) => "clipboard",
            AppError::Internal(_) => "internal",
        }
    }

    /// 利用者に見せるメッセージ。履歴の内容、コピー元、鍵は含めない。
    pub fn message(&self) -> String {
        match self {
            AppError::KeyUnavailable => {
                "暗号鍵を取得できないため、履歴を読み込んでいません。保存済みの履歴は変更していません".to_string()
            }
            AppError::KeyMismatch => {
                "暗号鍵が保存済みの履歴と合わないため、履歴を読み込んでいません。保存済みの履歴は変更していません".to_string()
            }
            AppError::Storage(m) => format!("履歴の保存でエラーが起きました: {m}"),
            AppError::Validation { message, .. } => message.clone(),
            AppError::ShortcutConflict(m) => format!("ショートカットを登録できません: {m}"),
            AppError::NotFound => "対象の履歴が見つかりません".to_string(),
            AppError::Clipboard(m) => format!("クリップボードの操作でエラーが起きました: {m}"),
            AppError::Internal(m) => format!("内部エラー: {m}"),
        }
    }

    /// 入力の検証エラーを作る。
    pub fn validation(field: &str, message: &str) -> Self {
        AppError::Validation {
            field: field.to_string(),
            message: message.to_string(),
        }
    }
}

impl fmt::Display for AppError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.message())
    }
}

impl std::error::Error for AppError {}

impl Serialize for AppError {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let field = match self {
            AppError::Validation { field, .. } => Some(field.as_str()),
            _ => None,
        };
        let mut s = serializer.serialize_struct("AppError", if field.is_some() { 3 } else { 2 })?;
        s.serialize_field("code", self.code())?;
        s.serialize_field("message", &self.message())?;
        if let Some(field) = field {
            s.serialize_field("field", field)?;
        }
        s.end()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn エラーはcodeとmessageのJSONになる() {
        let json = serde_json::to_value(AppError::NotFound).unwrap();
        assert_eq!(json["code"], "not_found");
        assert!(json["message"].is_string());
        assert!(json.get("field").is_none());
    }

    #[test]
    fn 検証エラーはfieldを含む() {
        let json = serde_json::to_value(AppError::validation(
            "max_items",
            "1以上の整数で入力してください",
        ))
        .unwrap();
        assert_eq!(json["code"], "validation");
        assert_eq!(json["field"], "max_items");
    }
}

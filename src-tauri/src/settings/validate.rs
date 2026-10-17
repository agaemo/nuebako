// 骨組み(P1-4)。本体は各トラックで実装し、P3-1 でこの属性を外す。
#![allow(dead_code, unused_variables)]

//! 設定の入力検証。

use crate::domain::clip::AppRef;
use crate::domain::settings::{MAX_ITEMS_LIMIT, RETENTION_DAYS_LIMIT};
use crate::error::AppError;

/// 最大件数と保存期間(文字列)を検証して数値にする。
pub fn validate_limits(max_items: &str, retention_days: &str) -> Result<(u32, u32), AppError> {
    let max = parse_limit("max_items", "最大件数", max_items, MAX_ITEMS_LIMIT)?;
    let days = parse_limit(
        "retention_days",
        "保存期間",
        retention_days,
        RETENTION_DAYS_LIMIT,
    )?;
    Ok((max, days))
}

/// 1 以上 `limit` 以下の半角整数だけを受け付ける(空白・全角・符号・小数はエラー)。
fn parse_limit(field: &str, label: &str, input: &str, limit: u32) -> Result<u32, AppError> {
    let message = format!("{label}は 1 以上 {limit} 以下の半角整数で入力してください");
    if input.is_empty() || !input.bytes().all(|b| b.is_ascii_digit()) {
        return Err(AppError::validation(field, &message));
    }
    match input.parse::<u32>() {
        Ok(n) if (1..=limit).contains(&n) => Ok(n),
        _ => Err(AppError::validation(field, &message)),
    }
}

/// 除外アプリの重複(同じ id)を、最初の1件にまとめる。
pub fn dedup_excluded_apps(apps: Vec<AppRef>) -> Vec<AppRef> {
    let mut seen = std::collections::HashSet::new();
    apps.into_iter()
        .filter(|a| seen.insert(a.id.clone()))
        .collect()
}

/// ショートカット文字列の形式を検証する(修飾キーを1つ以上と、通常のキーを1つ含む)。
pub fn validate_accelerator(accelerator: &str) -> Result<(), AppError> {
    const MODIFIERS: [&str; 11] = [
        "command",
        "cmd",
        "super",
        "meta",
        "control",
        "ctrl",
        "commandorcontrol",
        "cmdorctrl",
        "alt",
        "option",
        "shift",
    ];
    let invalid = || {
        AppError::validation(
            "shortcut",
            "修飾キー(Ctrl・Cmd・Alt・Shift)を1つ以上含めてください",
        )
    };
    let tokens: Vec<&str> = accelerator.split('+').collect();
    if tokens.iter().any(|t| t.is_empty()) {
        return Err(invalid());
    }
    let modifiers = tokens
        .iter()
        .filter(|t| MODIFIERS.contains(&t.to_ascii_lowercase().as_str()))
        .count();
    // 修飾キーが1つ以上あり、通常のキーがちょうど1つあること
    if modifiers == 0 || tokens.len() - modifiers != 1 {
        return Err(invalid());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn field_of(r: Result<(u32, u32), AppError>) -> String {
        match r {
            Err(AppError::Validation { field, .. }) => field,
            other => panic!("検証エラーのはず: {other:?}"),
        }
    }

    #[test]
    fn 正しい整数のとき数値になる() {
        assert_eq!(validate_limits("1000", "30").unwrap(), (1000, 30));
        assert_eq!(validate_limits("1", "1").unwrap(), (1, 1));
    }

    #[test]
    fn 上限ちょうどのとき受け付ける() {
        assert_eq!(validate_limits("100000", "3650").unwrap(), (100_000, 3_650));
    }

    #[test]
    fn 最大件数が不正のときmax_itemsのエラー() {
        for bad in [
            "0",
            "-1",
            "1.5",
            "",
            "abc",
            "１０００",
            " 10",
            "10 ",
            "100001",
            "+5",
            "99999999999999999999",
        ] {
            assert_eq!(
                field_of(validate_limits(bad, "30")),
                "max_items",
                "入力: {bad:?}"
            );
        }
    }

    #[test]
    fn 保存期間が不正のときretention_daysのエラー() {
        for bad in ["0", "-1", "1.5", "", "abc", "３０", "30 ", "3651"] {
            assert_eq!(
                field_of(validate_limits("1000", bad)),
                "retention_days",
                "入力: {bad:?}"
            );
        }
    }

    #[test]
    fn 両方不正のとき最大件数を先に報告する() {
        assert_eq!(field_of(validate_limits("0", "0")), "max_items");
    }

    fn app(id: &str, name: &str) -> AppRef {
        AppRef {
            id: id.to_string(),
            name: name.to_string(),
        }
    }

    #[test]
    fn 除外アプリの重複はまとめる() {
        let apps = vec![app("a", "A"), app("b", "B"), app("a", "A2")];
        assert_eq!(
            dedup_excluded_apps(apps),
            vec![app("a", "A"), app("b", "B")]
        );
    }

    #[test]
    fn 修飾キーとキーがあるときショートカットは正しい() {
        for ok in [
            "CommandOrControl+Shift+V",
            "Alt+Space",
            "Ctrl+F1",
            "Command+Shift+Alt+K",
        ] {
            assert!(validate_accelerator(ok).is_ok(), "{ok}");
        }
    }

    #[test]
    fn 修飾キーが無いときショートカットはエラー() {
        for bad in [
            "V",
            "F1",
            "",
            "Shift",
            "Ctrl+Shift",
            "Ctrl+",
            "Ctrl+A+B",
            "+",
        ] {
            assert!(
                matches!(validate_accelerator(bad), Err(AppError::Validation { ref field, .. }) if field == "shortcut"),
                "{bad:?}"
            );
        }
    }
}

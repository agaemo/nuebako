//! 検索(部分一致、英字の大文字小文字を無視)と、一覧用の文字列処理。
//! NFC 正規化は採用しない(既知の制約)。

/// 一覧の1行に出す先頭行の最大文字数。
pub const FIRST_LINE_MAX_CHARS: usize = 200;

/// 検索用に小文字へそろえる。
pub fn fold(s: &str) -> String {
    s.to_lowercase()
}

/// 検索語が本文に含まれるか。`haystack_folded` と `query` は `fold` 済みであること。空のクエリは常に一致。
pub fn matches(haystack_folded: &str, query: &str) -> bool {
    query.is_empty() || haystack_folded.contains(query)
}

/// 最初の改行までを、最大 200 文字で返す。`char` 単位なのでサロゲートペアの途中では切れない。
pub fn first_line(text: &str) -> String {
    let line = text.split(['\n', '\r']).next().unwrap_or("");
    line.chars().take(FIRST_LINE_MAX_CHARS).collect()
}

/// 空文字、空白のみ、ゼロ幅文字(U+200B・200C・200D・2060・FEFF)のみか。
pub fn is_blank(text: &str) -> bool {
    text.chars().all(|c| {
        c.is_whitespace()
            || matches!(
                c,
                '\u{200B}' | '\u{200C}' | '\u{200D}' | '\u{2060}' | '\u{FEFF}'
            )
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn 英字の大文字小文字を無視して部分一致する() {
        assert!(matches(&fold("Hello World"), &fold("lo wo")));
        assert!(matches(&fold("hello"), &fold("HELLO")));
        assert!(!matches(&fold("hello"), &fold("world")));
    }

    #[test]
    fn 日本語も部分一致する() {
        assert!(matches(&fold("これはテストです"), &fold("テスト")));
    }

    #[test]
    fn 空のクエリは常に一致する() {
        assert!(matches(&fold("abc"), ""));
    }

    #[test]
    fn 先頭行は最初の改行までになる() {
        assert_eq!(first_line("a\nb"), "a");
        assert_eq!(first_line("a\r\nb"), "a");
        assert_eq!(first_line("\nb"), "");
        assert_eq!(first_line("単一行"), "単一行");
    }

    #[test]
    fn 先頭行は200文字までになる() {
        let s = "あ".repeat(250);
        assert_eq!(first_line(&s).chars().count(), 200);
        let exact = "a".repeat(200);
        assert_eq!(first_line(&exact), exact);
    }

    #[test]
    fn 先頭行はサロゲートペアの途中で切らない() {
        // U+1F600 は UTF-16 ではサロゲートペア。200 文字目がこの文字でも、欠けずに残る
        let s = format!("{}😀{}", "a".repeat(199), "b".repeat(10));
        let r = first_line(&s);
        assert_eq!(r.chars().count(), 200);
        assert!(r.ends_with('😀'));
    }

    #[test]
    fn 空白とゼロ幅文字だけなら空とみなす() {
        assert!(is_blank(""));
        assert!(is_blank(" \t\r\n　"));
        for z in ['\u{200B}', '\u{200C}', '\u{200D}', '\u{2060}', '\u{FEFF}'] {
            assert!(is_blank(&z.to_string()), "{z:?}");
            assert!(is_blank(&format!(" {z} ")));
        }
        assert!(!is_blank(" a "));
        assert!(!is_blank("\u{200B}x"));
    }
}

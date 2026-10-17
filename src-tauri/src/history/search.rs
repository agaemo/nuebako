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

/// 一覧の1行に出す先頭行と、それより後ろの行数。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LinePreview {
    pub first_line: String,
    pub extra_lines: u32,
}

/// 一覧用の先頭行(空行と空白だけの行は飛ばした最初の行。最大 200 文字)と、その後ろの行数を返す。
/// 先頭行から制御文字と双方向制御文字を取り除くのは、表示の乗っ取り(右から左への並び替えなど)を防ぐため。
pub fn preview(text: &str) -> LinePreview {
    let mut first: Option<&str> = None;
    let mut extra = 0u32;
    // 空行が続いたあとに本文の行が来たときだけ、その空行を行数に数える(末尾の空行は数えない)
    let mut pending_blank = 0u32;
    for line in lines(text) {
        if first.is_none() {
            if !is_blank(line) {
                first = Some(line);
            }
        } else if is_blank(line) {
            pending_blank += 1;
        } else {
            extra += pending_blank + 1;
            pending_blank = 0;
        }
    }
    LinePreview {
        first_line: first.map(sanitize_line).unwrap_or_default(),
        extra_lines: extra,
    }
}

/// `\n`、`\r\n`、`\r` のどれでも行に分ける(確保なしで、先頭から順に返す)。
fn lines(text: &str) -> impl Iterator<Item = &str> {
    let mut rest = Some(text);
    std::iter::from_fn(move || {
        let s = rest?;
        match s.find(['\n', '\r']) {
            None => {
                rest = None;
                Some(s)
            }
            Some(i) => {
                let crlf = s[i..].starts_with("\r\n");
                rest = Some(&s[i + if crlf { 2 } else { 1 }..]);
                Some(&s[..i])
            }
        }
    })
}

/// 制御文字と双方向制御文字を取り除き、最大 200 文字にする。`char` 単位なのでサロゲートペアの途中では切れない。
/// タブは、前後の語がつながらないよう空白にする。
fn sanitize_line(line: &str) -> String {
    line.chars()
        .filter(|&c| c == '\t' || !(c.is_control() || is_bidi_control(c)))
        .map(|c| if c == '\t' { ' ' } else { c })
        .take(FIRST_LINE_MAX_CHARS)
        .collect()
}

/// 双方向制御文字(U+202A〜U+202E、U+2066〜U+2069)。
fn is_bidi_control(c: char) -> bool {
    matches!(c, '\u{202A}'..='\u{202E}' | '\u{2066}'..='\u{2069}')
}

/// 空白、または画面に何も出ない文字か。Unicode の空白に加えて、幅のない文字や
/// ハングル・点字の埋め字など、見た目が空白と区別できないものを含む。
fn is_invisible(c: char) -> bool {
    c.is_whitespace()
        || matches!(
            c,
            '\u{00AD}'
                | '\u{034F}'
                | '\u{115F}'
                | '\u{1160}'
                | '\u{17B4}'
                | '\u{17B5}'
                | '\u{180E}'
                | '\u{2000}'..='\u{200F}'
                | '\u{2028}'..='\u{202F}'
                | '\u{205F}'..='\u{206F}'
                | '\u{2800}'
                | '\u{3164}'
                | '\u{FEFF}'
                | '\u{FFA0}'
        )
}

/// 空文字、空白のみ、不可視文字のみか。
pub fn is_blank(text: &str) -> bool {
    text.chars().all(is_invisible)
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

    fn p(text: &str) -> (String, u32) {
        let r = preview(text);
        (r.first_line, r.extra_lines)
    }

    #[test]
    fn 先頭行は最初の改行までになり後ろの行数を数える() {
        assert_eq!(p("a"), ("a".into(), 0));
        assert_eq!(p("a\nb"), ("a".into(), 1));
        assert_eq!(p("a\r\nb\r\nc"), ("a".into(), 2));
        assert_eq!(p("a\rb"), ("a".into(), 1));
        assert_eq!(p("単一行"), ("単一行".into(), 0));
    }

    #[test]
    fn 先頭の空行と空白だけの行は飛ばす() {
        assert_eq!(p("\nb"), ("b".into(), 0));
        assert_eq!(p("\n  \n\t\nfoo\nbar"), ("foo".into(), 1));
        assert_eq!(p("\u{3164}\nfoo"), ("foo".into(), 0));
    }

    #[test]
    fn 末尾の空行は行数に数えず途中の空行は数える() {
        assert_eq!(p("a\n"), ("a".into(), 0));
        assert_eq!(p("a\n\n  \n"), ("a".into(), 0));
        assert_eq!(p("a\n\nb\n\n"), ("a".into(), 2));
    }

    #[test]
    fn 全部が空なら空の先頭行を返す() {
        assert_eq!(p(""), ("".into(), 0));
        assert_eq!(p(" \n\n"), ("".into(), 0));
    }

    #[test]
    fn 先頭行から制御文字と双方向制御文字を取り除く() {
        assert_eq!(p("a\u{0007}b\u{001B}c"), ("abc".into(), 0));
        assert_eq!(
            p("a\u{202E}b\u{2066}c\u{2069}d\u{202A}"),
            ("abcd".into(), 0)
        );
        // タブは単語がつながらないよう空白にする
        assert_eq!(p("a\tb"), ("a b".into(), 0));
    }

    #[test]
    fn 先頭行は200文字までになる() {
        let s = "あ".repeat(250);
        assert_eq!(p(&s).0.chars().count(), 200);
        let exact = "a".repeat(200);
        assert_eq!(p(&exact).0, exact);
    }

    #[test]
    fn 先頭行はサロゲートペアの途中で切らない() {
        // U+1F600 は UTF-16 ではサロゲートペア。200 文字目がこの文字でも、欠けずに残る
        let s = format!("{}😀{}", "a".repeat(199), "b".repeat(10));
        let r = p(&s).0;
        assert_eq!(r.chars().count(), 200);
        assert!(r.ends_with('😀'));
    }

    #[test]
    fn 空白と不可視文字だけなら空とみなす() {
        assert!(is_blank(""));
        assert!(is_blank(" \t\r\n　"));
        let invisibles = [
            '\u{200B}', '\u{200C}', '\u{200D}', '\u{2060}', '\u{FEFF}', '\u{3164}', '\u{2800}',
            '\u{00AD}', '\u{034F}', '\u{115F}', '\u{1160}', '\u{17B4}', '\u{17B5}', '\u{180E}',
            '\u{2000}', '\u{200F}', '\u{2028}', '\u{202A}', '\u{202F}', '\u{205F}', '\u{2066}',
            '\u{206F}', '\u{FFA0}',
        ];
        for z in invisibles {
            assert!(is_blank(&z.to_string()), "{z:?}");
            assert!(is_blank(&format!(" {z} ")));
        }
        assert!(!is_blank(" a "));
        assert!(!is_blank("\u{200B}x"));
        assert!(!is_blank("\u{3042}"));
    }
}

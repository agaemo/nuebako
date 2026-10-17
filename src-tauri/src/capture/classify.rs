//! 内容の種類の判定。優先順: 画像 > URL > パス > コード > テキスト。track-clipboard が実装する。
//!
//! 画像はクリップボードの形式で決まるため、このファイルが判定するのはテキストの種類だけ
//! (呼び出し側が画像を読めたときは `ClipKind::Image` を直接使う)。

use crate::domain::clip::ClipKind;

/// コードの先頭語として扱う語。
const CODE_HEAD_WORDS: &[&str] = &[
    "curl",
    "git",
    "npm",
    "pnpm",
    "yarn",
    "cargo",
    "docker",
    "cd",
    "const",
    "let",
    "var",
    "function",
    "import",
    "export",
    "def",
    "class",
    "fn",
    "pub",
    "use",
    "SELECT",
    "INSERT",
    "UPDATE",
    "DELETE",
    "#include",
    "#!/bin/sh",
    "#!/bin/bash",
    "sudo",
    "mise",
    "make",
];

/// 複数行のとき、コードとみなす記号の比率(空白を除く文字数に対する)。
const SYMBOL_RATIO_THRESHOLD: f64 = 0.12;

/// テキストの種類を判定する。優先順は URL > パス > コード > テキスト。
pub fn classify_text(text: &str) -> ClipKind {
    let trimmed = text.trim();
    if is_url(trimmed) {
        ClipKind::Url
    } else if is_path(trimmed) {
        ClipKind::Path
    } else if is_code(trimmed) {
        ClipKind::Code
    } else {
        ClipKind::Text
    }
}

/// `http://` または `https://` で始まり、続きがあり、空白を含まない1行。
fn is_url(s: &str) -> bool {
    let rest = s
        .strip_prefix("https://")
        .or_else(|| s.strip_prefix("http://"));
    match rest {
        Some(rest) => !rest.is_empty() && !s.chars().any(char::is_whitespace),
        None => false,
    }
}

/// `/`・`~/`・`C:\`・`\\server\` で始まる1行。`//` と `/*` で始まるものはコメントとして除く。
fn is_path(s: &str) -> bool {
    if s.is_empty() || s.contains('\n') || s.contains('\r') {
        return false;
    }
    if let Some(rest) = s.strip_prefix("~/") {
        return !rest.is_empty();
    }
    if let Some(rest) = s.strip_prefix('/') {
        return !rest.is_empty() && !rest.starts_with('/') && !rest.starts_with('*');
    }
    if let Some(rest) = s.strip_prefix("\\\\") {
        return !rest.is_empty();
    }
    // ドライブレター + ":\" (または ":/")
    let mut chars = s.chars();
    if let (Some(d), Some(':'), Some(sep)) = (chars.next(), chars.next(), chars.next()) {
        return d.is_ascii_alphabetic() && (sep == '\\' || sep == '/') && chars.next().is_some();
    }
    false
}

/// 先頭語がコマンド・予約語、または複数行で記号の比率が高い。
fn is_code(s: &str) -> bool {
    let head = s.split_whitespace().next().unwrap_or("");
    if CODE_HEAD_WORDS.contains(&head) {
        return true;
    }
    if !s.contains('\n') {
        return false;
    }
    let mut symbols = 0usize;
    let mut total = 0usize;
    for c in s.chars().filter(|c| !c.is_whitespace()) {
        total += 1;
        if "{}()[];<>=&|$\\*+/_#".contains(c) {
            symbols += 1;
        }
    }
    total > 0 && (symbols as f64 / total as f64) >= SYMBOL_RATIO_THRESHOLD
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn httpsで始まる1行のときURLになる() {
        assert_eq!(classify_text("https://example.com/a?b=1"), ClipKind::Url);
    }

    #[test]
    fn httpで始まる1行のときURLになる() {
        assert_eq!(classify_text("http://example.com"), ClipKind::Url);
    }

    #[test]
    fn 前後に空白があってもURLになる() {
        assert_eq!(classify_text("  https://example.com \n"), ClipKind::Url);
    }

    #[test]
    fn 途中に空白を含むとURLにならない() {
        assert_eq!(classify_text("https://example.com is good"), ClipKind::Text);
    }

    #[test]
    fn スキームだけのときURLにならない() {
        assert_eq!(classify_text("http://"), ClipKind::Text);
        assert_eq!(classify_text("https://"), ClipKind::Text);
    }

    #[test]
    fn スラッシュで始まる1行のときパスになる() {
        assert_eq!(classify_text("/usr/local/bin"), ClipKind::Path);
    }

    #[test]
    fn チルダスラッシュで始まるときパスになる() {
        assert_eq!(classify_text("~/Documents/a.txt"), ClipKind::Path);
    }

    #[test]
    fn ドライブレターで始まるときパスになる() {
        assert_eq!(classify_text("C:\\Users\\me\\a.txt"), ClipKind::Path);
    }

    #[test]
    fn UNCパスのときパスになる() {
        assert_eq!(classify_text("\\\\server\\share\\a.txt"), ClipKind::Path);
    }

    #[test]
    fn 空白を含む1行のパスもパスになる() {
        assert_eq!(
            classify_text("/Users/me/My Documents/a.txt"),
            ClipKind::Path
        );
    }

    #[test]
    fn スラッシュだけのときパスにならない() {
        assert_eq!(classify_text("/"), ClipKind::Text);
        assert_eq!(classify_text("~/"), ClipKind::Text);
    }

    #[test]
    fn 複数行はパスにならない() {
        assert_ne!(classify_text("/usr/bin\n/usr/lib"), ClipKind::Path);
    }

    #[test]
    fn コメント記号で始まる行はパスにならない() {
        assert_ne!(classify_text("// 説明"), ClipKind::Path);
        assert_ne!(classify_text("/* 説明 */"), ClipKind::Path);
    }

    #[test]
    fn コマンドで始まるときコードになる() {
        assert_eq!(classify_text("git commit -m \"x\""), ClipKind::Code);
        assert_eq!(classify_text("curl -s https://example.com"), ClipKind::Code);
        assert_eq!(classify_text("cd ~/work"), ClipKind::Code);
        assert_eq!(classify_text("pnpm install"), ClipKind::Code);
        assert_eq!(classify_text("docker ps"), ClipKind::Code);
        assert_eq!(classify_text("cargo test"), ClipKind::Code);
        assert_eq!(classify_text("npm run build"), ClipKind::Code);
    }

    #[test]
    fn 予約語で始まるときコードになる() {
        assert_eq!(classify_text("const a = 1;"), ClipKind::Code);
        assert_eq!(classify_text("let x = 1"), ClipKind::Code);
        assert_eq!(classify_text("function f() {}"), ClipKind::Code);
        assert_eq!(classify_text("import os"), ClipKind::Code);
        assert_eq!(classify_text("def f():"), ClipKind::Code);
        assert_eq!(classify_text("class A {}"), ClipKind::Code);
        assert_eq!(classify_text("fn main() {}"), ClipKind::Code);
        assert_eq!(classify_text("pub fn a() {}"), ClipKind::Code);
        assert_eq!(classify_text("SELECT * FROM t"), ClipKind::Code);
        assert_eq!(classify_text("#include <stdio.h>"), ClipKind::Code);
    }

    #[test]
    fn 複数行で記号の比率が高いときコードになる() {
        let text = "if (a && b) {\n  x[0] = y;\n}\n";
        assert_eq!(classify_text(text), ClipKind::Code);
    }

    #[test]
    fn 日本語の文章のときテキストになる() {
        assert_eq!(
            classify_text("今日は晴れです。\n明日は雨でしょう。"),
            ClipKind::Text
        );
        assert_eq!(classify_text("こんにちは"), ClipKind::Text);
    }

    #[test]
    fn 記号の少ない英語の複数行はテキストになる() {
        assert_eq!(
            classify_text("Hello there.\nHow are you today?"),
            ClipKind::Text
        );
    }

    #[test]
    fn 優先順はURLがパスとコードより先() {
        assert_eq!(classify_text("https://example.com/usr/bin"), ClipKind::Url);
    }
}

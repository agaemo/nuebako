#!/bin/sh
# PostToolUse hook: 書き込んだファイルを整形する(.rs は cargo fmt、.ts/.tsx は oxfmt)。
# 失敗しても作業を止めない(終了コードは常に 0)。
input=$(cat)
file=$(printf '%s' "$input" | sed -n 's/.*"file_path"[[:space:]]*:[[:space:]]*"\([^"]*\)".*/\1/p' | head -n 1)
[ -z "$file" ] && exit 0
cd "${CLAUDE_PROJECT_DIR:-.}" || exit 0

case "$file" in
  *.rs)
    mise exec -- cargo fmt --manifest-path src-tauri/Cargo.toml --all >/dev/null 2>&1
    ;;
  *.ts|*.tsx)
    mise exec -- pnpm exec oxfmt "$file" >/dev/null 2>&1
    ;;
esac
exit 0

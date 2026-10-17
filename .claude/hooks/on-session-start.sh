#!/bin/sh
# SessionStart hook: git ブランチ・未コミット変更を表示し、mise install を実行する
cat > /dev/null

branch=$(git branch --show-current 2>/dev/null) || exit 0
printf 'Git ブランチ: %s\n' "$branch"

status=$(git status --short 2>/dev/null)
if [ -n "$status" ]; then
  printf '未コミットの変更:\n%s\n' "$status"
fi

if [ -f .mise.toml ]; then
  mise install --quiet 2>/dev/null || printf 'WARNING: mise が使えません。インストールしてください: https://mise.jdx.dev\n'
fi

exit 0

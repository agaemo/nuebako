#!/bin/sh
# PreToolUse hook: Bash — 危険なコマンドをブロックする
# exit 0 → 許可 / exit 2 → ブロック
input=$(cat)

block() {
  printf 'pre-bash フックによりブロックされました: %s\n' "$1"
  exit 2
}

echo "$input" | grep -qE 'rm[[:space:]]+-rf[[:space:]]+/' && block 'rm -rf /'
echo "$input" | grep -qE 'git[[:space:]]+push[[:space:]]+--force' && block 'git push --force'
echo "$input" | grep -qE 'git[[:space:]]+reset[[:space:]]+--hard' && block 'git reset --hard'
echo "$input" | grep -qiE 'DROP[[:space:]]+TABLE' && block 'DROP TABLE'
echo "$input" | grep -qE '>[[:space:]]*\.env' && block '> .env'

exit 0

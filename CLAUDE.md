# nuebako

クリップボードの履歴を暗号化してローカルに保存し、ショートカットで呼び出して検索・貼り付けする常駐デスクトップアプリ(macOS Apple Silicon と Windows x64)。個人・職場での利用向けで、配布は GitHub Releases(署名なし。いまは macOS のみで、Windows は実機での確認ができるまで配布しない)。設計と実装計画は `.craft/docs/plan.md`(git 管理外)にある。

## スタック

- Tauri 2.12(Rust 1.99、`panic = "abort"` は使わない)、React 19 + TypeScript + Vite 8、Tailwind CSS v4(UI ライブラリなし)
- 保存: rusqlite(bundled)。1件ずつ AES-256-GCM で暗号化し、鍵は OS のキーチェーン(keyring-core)に保存する
- クリップボードの監視と書き込みは clipboard-rs、自動貼り付けは enigo(どちらも 1.0 未満で、バージョンを固定)
- ツールは mise で固定する(`.mise.toml`)

## コマンド(必ず `make` 経由。`cargo`・`pnpm` を直接呼ばない)

- `make dev` 開発起動 / `make build` / `make test`(cargo test と Vitest)/ `make lint`(clippy、tsc、oxlint)/ `make fmt`
- `make bundle` インストーラ作成 / `make licenses` 依存のライセンス一覧 / `make icons` アイコン一式の生成
- 起動・動作確認の依頼では、`/run` などのスキルではなく、上のコマンドを直接使う
- 表に無いコマンドが必要になったら、直接実行せず、Makefile にターゲットを追加してから使う

## 構成と依存の向き

- Rust(`src-tauri/src`): `commands`(IPC。引数の検証だけ)→ `history`・`settings`・`paste`・`capture`(サービス)→ `storage`・`crypto`・`platform`(アダプタ)。OS 依存や 1.0 未満の部品は trait で包む
- 履歴を保存する入口は `HistoryService::record` だけ。`capture` は storage を直接呼ばない
- フロントエンド(`src`): `features/history`・`features/settings`(画面)、`components/ui`(共通部品)、`lib/ipc.ts`(Rust 呼び出しはここだけ)。色は `src/styles/tokens.css` のトークンを使い、直書きしない
- 画面は小型(幅 420px、本文 12px)。インライン `style` は使わない(本番の CSP が拒否する)
- ウィンドウは `history`(枠なし)と `settings`。ウィンドウを閉じる操作は破棄せず隠す。終了はトレイの「終了」だけ

## 守ること

- 履歴の内容、コピー元、暗号鍵を、ログやエラーに出さない。テストにも実データを使わない
- 鍵が取れないときは、読み込まず、書き込まず、鍵も作り直さない(履歴を壊さないため)
- macOS の動作は実機で確認する。Windows は CI(`ci.yml` の手動実行でインストーラを作れる)と実機で確認する
- 日付・時刻のテストは時計を固定する。非同期の一覧は、古い応答を捨てる(`req_id`)
- 公開リポジトリなので、社内の情報、絶対パス、ユーザー名を書かない
- 方式の決定は `docs/adr/`(保存と暗号化、クリップボード監視、ウィンドウのフォーカス)

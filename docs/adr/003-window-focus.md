# ADR-003: 履歴ウィンドウの前面化と元のアプリへのフォーカス復帰の方式

**日付:** 2026-10-17
**ステータス:** 決定済み(macOS はスパイクで確認済み。Windows は実機で未確認)

## 背景
グローバルショートカットで履歴ウィンドウを他のアプリの前面に出し、選択後はウィンドウを閉じて直前のアプリにフォーカスを戻し、自動で貼り付ける(US-003・US-004)。トレイ常駐のため、macOS では Dock にアイコンを出したくない。Tauri 公式にはこの用途の専用 API が無く、通常ウィンドウでは難しいという議論がある(tech-research.md 参照)。

## 決定
- 通常の Tauri ウィンドウ(枠なし・最前面・タスクバー非表示・420px 固定幅)を使う。NSPanel(tauri-nspanel)は使わない。
- macOS: 起動時に `App::set_activation_policy(ActivationPolicy::Accessory)` で Dock に出さない。ウィンドウを閉じるときは、ウィンドウを隠したうえで `AppHandle::hide()` でアプリごと隠す。すると OS が直前のアプリにフォーカスを戻す。
- Windows: 表示前に `GetForegroundWindow` で最前面のウィンドウを記録し、閉じた後に `SetForegroundWindow` で戻す(`windows` crate 0.62。コードは Windows ターゲットでコンパイル確認済み。動作は実機で未確認)。
- フォーカスが外れたとき(外クリック)に隠す。表示直後 400ms は、フォーカスの変動を無視する。
- 閉じる操作(CloseRequested)は破棄せず隠す。
- 貼り付けは、ウィンドウを閉じた後、100ms を待ってから Cmd/Ctrl+V を送る。スパイクでは 0ms でも成功した。実機の確認表(M-4)で調整する。

## スパイクの結果(macOS、2026-10-17、開発ビルド)
| 確認項目 | 結果 |
|---|---|
| ショートカットで他アプリの前面に出て、検索欄に入力できる | OK |
| Esc、再ショートカット、外クリックで閉じ、直前のアプリにフォーカスが戻る | OK |
| Dock にアイコンが出ない(`set_focus()` 後、隠した後も) | OK |
| 閉じた直後の Cmd+V が、直前のアプリに貼り付く(待機 0、100、250、500ms) | すべて OK |
| アクセシビリティ権限の判定(`AXIsProcessTrusted`) | OK(権限ありで true) |
| クリップボードの変更を検知できる | OK(テキスト、HTML、画像) |
| 機密の型名(`org.nspasteboard.ConcealedType`)と `source` を読める | OK(疑似コピーで確認。実際のパスワード管理アプリでは未確認) |
| コピー元アプリ | `org.nspasteboard.source` は Ghostty、Chrome では設定されない。最前面アプリは取得できる(自分が最前面のときは、開発ビルドで bundle id が空) |

追加で分かったこと:
- `AppHandle::set_dock_visibility(false)` も存在する(調査では「見つからなかった」とされていた)。今回は Accessory で足りたため使わない。
- 最前面アプリの取得は、スパイクでは `lsappinfo` コマンドで行ったが、本実装では `objc2` と `objc2-app-kit`(macOS)、`windows`(Windows)で行う。
- clipboard-rs の `set_text` / `set_buffer` は、呼ぶたびにクリップボード全体を置き換える。複数の形式を書くときは `set(Vec<ClipboardContent>)` を使う。

## 理由
- 依存を増やさず、Tauri 公式の範囲で要件を満たせた。
- tauri-nspanel は crates.io 公開版ではなく git ブランチ参照で、単独メンテナのため、必要になるまで導入しない。

## 却下した代替案
- 最初から tauri-nspanel を使う: スパイクで通常ウィンドウが要件を満たしたため不要。
- フォーカスを戻さず再コピーだけにする: 要件(自動ペースト)を満たさない。

## 影響
- Tauri アプリは起動時に他アプリのフォーカスを奪う(issue #15017、未解決)。起動時(ログイン時)の一瞬に限られるため許容する。
- Windows の `SetForegroundWindow` には、フォアグラウンドロックの制約がある。実機で貼り付けが失敗する場合は、ADR を見直す(`AllowSetForegroundWindow` や入力の送信での回避など)。
- macOS で署名なしのアプリを `.app` として配布したときの Accessory とフォーカス復帰の動作は、M-3(手動確認)で確認する。

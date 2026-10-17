# nuebako

クリップボードの履歴を保存し、ショートカットで呼び出して貼り付けられる常駐アプリです(macOS・Windows)。Tauri 2 + React 製で、履歴は暗号化してローカルにだけ保存します。

## ライセンス

[MIT License](LICENSE)(Copyright (c) 2026 agaemo)

## インストール

1. [GitHub Releases](../../releases) から、お使いの OS 用のインストーラをダウンロードします。
   - macOS(Apple Silicon): `.dmg`
   - Windows: `.exe`
2. macOS は `.dmg` を開いて `nuebako.app` を「アプリケーション」へドラッグします。Windows は `.exe` を実行します。

### 署名なしで配布している理由と、警告の回避手順

macOS の署名・公証には Apple Developer Program(有償)、Windows のコード署名には証明書(有償)が必要です。個人開発のため、現時点では署名なしで配布しています。そのため、初回起動時に OS が警告を出します。インストーラは GitHub Actions がソースからビルドしたものです。

**macOS**

1. `nuebako.app` を開こうとして警告が出たら、一度閉じます。
2. 「システム設定 > プライバシーとセキュリティ」を開き、下の方に表示される nuebako の項目で「このまま開く」を押します。
3. 「壊れているため開けません」と表示されて、上の手順でも開けない場合は、ターミナルで次を実行します。

   ```sh
   xattr -dr com.apple.quarantine /Applications/nuebako.app
   ```

   この `xattr` の手順は、Tauri の公式ドキュメントには記載がなく、実績ベースの回避策です(ダウンロードしたファイルに付く隔離属性を外します)。自己責任で実行してください。

**Windows**

SmartScreen の「Windows によって PC が保護されました」が出たら、「詳細情報」を押し、「実行」を選びます。

## 使い方

- **履歴ウィンドウ(S1)**: ショートカットで呼び出します。検索欄にはすぐ入力できます。一覧から選んで Enter(またはクリック)すると、直前に使っていたアプリへ貼り付けます。Tab で、選択中の履歴の全文を見られるプレビューを開閉できます。
- **設定ウィンドウ(S2)**: 呼び出しショートカット、テーマ、保存の上限・期間、除外アプリを設定します。履歴ウィンドウのフッターの「設定」、トレイメニュー、または Cmd/Ctrl + `,` で開きます。
- **トレイメニュー(S3)**: 常駐の入口です。「履歴を開く」「設定」「終了」があります。ウィンドウを閉じても、記録は続きます。終了するのは、トレイの「終了」を選んだときだけです。
- **番号貼り付け**: 一覧の行頭の番号(1〜9、10 行目は 0)を、macOS は Cmd、Windows は Alt と一緒に押すと、その行を貼り付けます。修飾キーなしの数字は検索欄への入力になります。

### ショートカット一覧

| 操作 | macOS | Windows |
|---|---|---|
| 履歴ウィンドウを呼び出す(既定。設定で変更可) | Cmd + Shift + V | Ctrl + Shift + V |
| 選択を移動 | ↑ ↓ | ↑ ↓ |
| 貼り付け | Enter | Enter |
| プレビューの開閉 | Tab | Tab |
| 閉じる | Esc | Esc |
| 番号で貼り付け | Cmd + 数字 | Alt + 数字 |
| 設定を開く | Cmd + , | Ctrl + , |

### アクセシビリティ権限(macOS)

他のアプリへ自動で貼り付けるには、「システム設定 > プライバシーとセキュリティ > アクセシビリティ」で nuebako の許可が必要です。許可しない場合は、選んだ内容をクリップボードに戻すだけ(再コピーのみ)になり、貼り付けは手動で行います。

アプリを更新したあとに、許可が外れて、再度の許可が必要になる場合があります(署名なしのため)。貼り付けが効かなくなったら、アクセシビリティの一覧で nuebako を確認してください。

## データの保存と暗号化

- 履歴の本文・画像・コピー元アプリ名などは、AES-256-GCM で暗号化して SQLite のファイル `nuebako.db` に保存します。平文で残るのは日時だけです(並べ替えと保存期間の判定に使います)。
- 暗号鍵は OS のキーチェーン(macOS: キーチェーン、Windows: 資格情報マネージャー)に保存します。鍵はファイルに書きません。
- 保存場所は、Tauri の `app_data_dir`(識別子 `io.github.agaemo.nuebako`)配下の `nuebako.db` です。OS ごとの一般的な場所は、macOS が `~/Library/Application Support/` 配下、Windows が `%APPDATA%` 配下です(正確なパスは今後確認して追記します)。
- 鍵が取得できない場合(キーチェーンのリセットなど)は、履歴を読み込まず、書き込まず、鍵も作り直しません。保存済みのファイルは変更されません。

## 機密情報の扱い

- **除外アプリ**: 設定にあるアプリからのコピーは記録しません。初期値は 1Password、Bitwarden、KeePassXC です。「実行中のアプリから追加」で増やせます。
- **機密フォーマット**: コピー元のアプリが「機密(パスワードなど)」の印を付けたコピーは記録しません。
- **検知できないケース**: コピー元が機密の印を付けない場合(ブラウザのパスワード管理拡張など)は、機密かどうかを判別できず、記録されます。そのようなアプリは除外アプリに追加してください。履歴は設定の「全履歴を削除」でいつでも消せます。

## 既知の制約

- 検索では Unicode の NFC 正規化を行いません。macOS 由来の NFD 形式の文字列(濁点・半濁点が分離した形のファイル名など)は、見た目が同じでも、検索語と一致しない場合があります。
- Windows は実機での確認が限定的です。特にフォーカスの復帰と自動貼り付けは、環境によって動作が異なる可能性があります。管理者権限で動いているウィンドウへは貼り付けられない場合があります(再コピーのみ行われます)。
- 動作確認した macOS のバージョン: (今後追記します)

## 開発者向け

ツールは [mise](https://mise.jdx.dev/) で管理します。コマンドは `make` 経由で実行します。

```sh
mise install
mise exec -- pnpm install --frozen-lockfile
make dev       # 開発用に起動
make build     # フロントエンドと Rust をビルド
make test      # Rust と Vitest のテスト
make lint      # clippy / tsc / oxlint
make fmt       # フォーマット
make bundle    # インストーラを作成
make licenses  # 依存ライブラリのライセンス一覧
```

### リリース手順

1. `package.json`、`src-tauri/tauri.conf.json`、`src-tauri/Cargo.toml` の `version` を揃えて更新し、コミットします。
2. `v` を付けたタグ(例: `v0.1.0`)を打って push します。
3. GitHub Actions(`release.yml`)が、タグと 3 つの `version` の一致を確認し、macOS(Apple Silicon)と Windows のインストーラをビルドして、**下書き**のリリースに添付します。
4. 全ジョブが成功すると、最後のジョブが下書きを公開します。いずれかが失敗した場合は、下書きのまま残り、公開されません。

同じタグでやり直す場合は、v1 の `tauri-action` が下書きでない既存リリースを受け付けないため、既存のリリースを下書きに戻すか、削除してから再実行してください。

## 主要ライブラリのライセンス

いずれも MIT と矛盾するもの(GPL 系のみのライセンス)はありません。一覧は `make licenses` で確認できます(`cargo metadata` の license 欄と `pnpm licenses list`)。

| ライブラリ | 用途 | ライセンス |
|---|---|---|
| tauri / tauri-plugin-global-shortcut / tauri-plugin-single-instance | アプリ基盤 | Apache-2.0 OR MIT |
| rusqlite(SQLite を同梱) | 履歴の保存 | MIT |
| aes-gcm | 暗号化 | Apache-2.0 OR MIT |
| keyring-core / apple-native-keyring-store / windows-native-keyring-store | 鍵の保管 | MIT OR Apache-2.0 |
| clipboard-rs | クリップボードの監視・書き込み | MIT |
| enigo | 貼り付けの操作 | MIT |
| serde / serde_json | シリアライズ | MIT OR Apache-2.0 |
| React / React DOM | UI | MIT |
| @tauri-apps/api | フロントエンドと Rust の連携 | Apache-2.0 OR MIT |
| Tailwind CSS(ビルド時) | スタイル | MIT |
| Shippori Mincho(同梱フォント) | ブランド名の書体 | OFL-1.1 |
| JetBrains Mono(同梱フォント) | 等幅の書体 | OFL-1.1 |

推移的な依存には、MPL-2.0(`cssparser`、`selectors` など。ファイル単位のコピーレフトで、未改変のまま利用)、BSL-1.0(`clipboard-win` など)、Unicode-3.0、BSD 系、ISC などの許容的なライセンスが含まれます。`r-efi` は `MIT OR Apache-2.0 OR LGPL-2.1-or-later` の選択式で、MIT を選べます。

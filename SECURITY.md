# セキュリティ

## 脆弱性の報告

脆弱性を見つけた場合は、公開のイシューではなく、GitHub の Private vulnerability reporting(リポジトリの「Security」タブ > 「Report a vulnerability」)から報告してください。再現手順と影響範囲が分かると助かります。個人開発のため、対応に時間がかかることがあります。

## 対応範囲

- 対象: 本リポジトリのソースコードと、GitHub Releases で配布するインストーラ(macOS Apple Silicon、Windows x64)
- 対象外: 次のものは、設計上の制約として README の「既知の制約」「機密情報の扱い」に記載しています。
  - OS やクリップボードの仕様で検知できない機密情報のコピー
  - プロセスのメモリや、ログイン中のユーザー権限を持つ攻撃者による読み取り
  - 依存ライブラリ自体の脆弱性(該当ライブラリへ報告してください。本アプリでの影響があれば、こちらにも報告してください)

## 署名なしの配布について

インストーラには、コード署名も公証も付けていません。そのため、初回起動時に OS が警告を出します(手順は README の「インストール」を参照)。代わりに、次の方法で、GitHub Actions がこのリポジトリのソースからビルドしたものであることを確認できます。

- リリースに添付した `SHA256SUMS` と、ダウンロードしたファイルのハッシュを比べる
- ビルドの証明書(provenance)を検証する: `gh attestation verify <ダウンロードしたファイル> -R agaemo/nuebako`

## 依存の更新

Dependabot が、Rust(cargo)、npm、GitHub Actions の依存の更新を、週次で pull request にします。更新の pull request は、CI(ビルドとテスト)が両 OS で通ることを確認してから、マージします。GitHub Actions は commit SHA で固定しているため、Dependabot の更新でも SHA が更新されることを確認します。

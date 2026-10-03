# Animeta

Annictの作品・エピソード情報を使って、アニメのMP4・MKVをリネームし、動画内にメタデータを付与するデスクトップアプリです。既定では元動画を保持して出力先に作成します。設定で「上書き」を選ぶと、元のフォルダで元動画を置き換えます。

Vue 3 / TypeScript / shadcn-vue / Tailwind CSS / Noto Sans JP Variable と、Tauri 2 / Rustを使用しています。

## ダウンロード

[GitHub Releases](https://github.com/minittupoyo/animeta/releases)から、Windows x64（FFmpeg同梱／非同梱ZIP）、Linux x86_64（AppImage／deb）を取得できます。公開済みリリースがない場合はソースからビルドしてください。配布形式・リリース手順は[リリース文書](docs/releases.md)を参照してください。

## 開発用の起動

bun、Rust、お使いのOSの[Tauri開発用依存関係](https://v2.tauri.app/start/prerequisites/)を用意してください。

```sh
bun install
bun run tauri dev
```

動画へのタグ付与にはFFmpegとffprobeが必要です。Windowsの同梱版は付属ツールを既定設定で使用します。非同梱版・Linux版は別途用意し、PATHまたは設定画面で実行ファイルを指定してください。タグ付与・字幕削除・チャプター削除を行わないリネーム・コピー処理では外部ツールは不要です。

Annictの[個人用アクセストークン](https://annict.com/settings/apps)を読み取り権限で作成し、設定画面から接続してください。トークンはOSの資格情報ストアに保存します。利用できない環境では起動中のみ保持します。

## 使い方

1. Annictで作品を検索して選択します。
2. ファイル、フォルダ、ドラッグ＆ドロップで動画を追加します。
3. ファイル名の話数から自動マッチしたエピソードを確認します。未割り当ては「自動マッチ」で再照合できます。各行の編集ボタンで話数・サブタイトル・タグを確認できます。
4. 設定の「リネーム方法」でコピー／上書きを選択します。コピーは出力先を指定し、上書きは元のフォルダで処理します。命名テンプレートを設定し、プレビューを生成します。
5. 必要に応じて「字幕を削除」「チャプターを削除」を選び、プレビューを生成します。どちらも既定ではオフです。字幕削除は内蔵字幕すべてが対象で、焼き込み字幕は削除できません。
6. 出力名と処理内容を確認して実行します。重複・未割り当てなどのエラーがある動画は除外されます。
7. 履歴で結果を確認し、失敗した動画は再取り込みできます。

既定の命名テンプレートは `{work_title}[ - {episode_label}][ - {episode_title}]` です。角括弧内は値がある場合のみ出力します。拡張子は自動で付加します。映画・特番は「作品単位」を選択できます。

操作方法は画面右上の「ヘルプ」、命名・接続・動画処理ツールの補足は各項目の「?」から確認できます。

ライト・ダーク・システム追従に対応します。サブフォルダ取り込みは設定で有効にできます。タグ付与は再エンコードなしの再多重化で行い、元のコンテナを保持します。特定プレイヤーでの表示互換性を保証するものではありません。

## ブラウザで画面を確認

```sh
bun run dev
```

ブラウザの「サンプルで試す」から、架空の作品を使った対応付け・命名・プレビューを確認できます。ブラウザでは実際のAnnict接続やファイル処理を行いません。サンプルデータはデスクトップアプリでは使用しません。

## 検証・ビルド

```sh
bun run check
bun run test
bun run build
bunx playwright install chromium
bun run test:ui
cargo fmt --manifest-path src-tauri/Cargo.toml --check
cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings
cargo test --manifest-path src-tauri/Cargo.toml
bun run tauri build --no-bundle
```

メディア結合テストにはFFmpeg・ffprobeを用意してください。テスト専用パスを使う場合は `ANIMETA_TEST_FFMPEG` と `ANIMETA_TEST_FFPROBE` を指定します。未インストールの場合、メディア結合テストはスキップ理由を出力します。UIテストにはPlaywright対応のブラウザ依存ライブラリも必要です。

### LinuxからWindows向けにビルド

[cargo-xwin](https://github.com/rust-cross/cargo-xwin)とClang / LLDを用意し、次のコマンドでWindows x64向けのリリース実行ファイルを生成できます。フロントエンドのビルドも実行されます。

```sh
cargo install cargo-xwin --locked
rustup target add x86_64-pc-windows-msvc
bun run tauri build --runner cargo-xwin --target x86_64-pc-windows-msvc --no-bundle
```

出力先は `src-tauri/target/x86_64-pc-windows-msvc/release/animeta.exe` です。インストーラーは生成しません。実行環境にはMicrosoft Edge WebView2 Runtimeと、動画のタグ付与に使うWindows版FFmpeg・ffprobeが必要です。

設定と履歴はOSごとのアプリデータフォルダ内の `net.minittu.animeta/animeta.sqlite3` に保存します。トークンはSQLiteへ保存しません。

## CI

GitHub Actionsで型チェック、フロントエンド・UI・Rustテスト、ビルドを実行します。Windows向けの `cargo-xwin` ビルドはActionsの「Windows build」から手動実行でき、生成した実行ファイルをartifactとして取得できます。タグから両OSの配布物を公開する「Release」ワークフローも用意しています。ブランチでの手動実行では公開せず、配布物をartifactとして生成します。

## ライセンス

[MIT License](LICENSE)。フォント・アイコン・UIコンポーネントの表記は[Third-party notices](THIRD_PARTY_NOTICES.md)を参照してください。

## 開発文書

- [仕様書](docs/specification.md)
- [アーキテクチャと検証手順](docs/architecture.md)
- [今回の検証結果・未検証項目](docs/verification.md)
- [リリース手順](docs/releases.md)
- [開発規約](AGENTS.md)

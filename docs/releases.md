# リリース

[Releaseワークフロー](../.github/workflows/release.yml)はWindows x64とLinux x86_64をビルドします。WindowsアプリはLinux上でcargo-xwinを使い、`x86_64-pc-windows-msvc`を対象にします。LinuxはUbuntu 22.04上でビルドします。

## 配布物

| ファイル | 内容 |
| --- | --- |
| `animeta-VERSION-windows-x64.zip` | Windowsアプリ、FFmpeg・ffprobe非同梱 |
| `animeta-VERSION-windows-x64-with-ffmpeg.zip` | Windowsアプリ、FFmpeg・ffprobe同梱 |
| `animeta-VERSION-linux-amd64.deb` | Debian系向けパッケージ。FFmpegをパッケージ依存関係として指定 |
| `animeta-VERSION-linux-x86_64.AppImage` | Linux向けAppImage。FFmpeg・ffprobeは別途用意 |
| `animeta-VERSION-notices.tar.gz` | ライセンス表記 |
| `SHA256SUMS.txt` | 配布物のSHA-256 |

WindowsはZIPをすべて展開して `animeta.exe` を起動します。両版ともMicrosoft Edge WebView2 Runtimeが必要です。同梱版は設定のパスが既定値 `ffmpeg` / `ffprobe`（`.exe`付きも可）の場合、アプリと同じフォルダの `tools/` を優先します。指定済みのカスタムパスは変更しません。同梱ツールの絶対パスを設定へ保存しないため、展開先を移動しても既定設定で使えます。非同梱版はPATHまたは設定のパスで外部ツールを指定してください。

AppImageは実行権限を付けて起動します。Ubuntu 22.04のglibc / WebKitGTK 4.1をビルド基準とし、すべてのLinuxディストリビューションでの互換性は保証しません。

```sh
chmod +x animeta-VERSION-linux-x86_64.AppImage
./animeta-VERSION-linux-x86_64.AppImage
# FUSEを使えない場合
APPIMAGE_EXTRACT_AND_RUN=1 ./animeta-VERSION-linux-x86_64.AppImage
```

## 公開

1. `package.json`、`src-tauri/tauri.conf.json`、`src-tauri/Cargo.toml` のバージョンを同じ値へ更新します。`bun install` とRustのテストを実行し、更新されたlockfileもコミットします。
2. テストを確認し、変更をmainへpushします。
3. 対応する `vVERSION` タグをpushします。タグとアプリのバージョンが不一致ならビルド前に失敗します。

```sh
# 現在のバージョンの例。実際に公開するときに実行してください。
git tag v0.1.0
git push origin v0.1.0
```

両OSのビルドとWindows上の起動・付属ツール検証、Linuxの起動検証に成功した後、配布物をまとめ、SHA-256を生成してGitHub Releaseへ公開します。`v0.1.0-rc.1` のようなハイフン付きバージョンはprereleaseになります。自動生成のリリースノートを使用します。失敗時にはReleaseを作成しません。同名Releaseがすでに存在する場合は失敗し、既存の配布物を置き換えません。

## 公開せずに検証

Actions画面の「Release」→「Run workflow」、または次のコマンドで実行します。手動実行はブランチを選択してください。

```sh
gh workflow run release.yml --ref main
```

ブランチ上の手動実行ではGitHub Releaseを作成せず、`release-assets` artifactにWindows・Linuxの配布物とSHA-256をまとめます。artifactの保持期間は14日です。タグを指定した手動実行は公開処理も実行するため、公開なしの検証ではmainなどのブランチを指定してください。

## FFmpeg同梱版のビルド

[ビルドスクリプト](../scripts/build-ffmpeg-windows.sh)はFFmpeg 8.1.3の[公式ソース](https://ffmpeg.org/releases/ffmpeg-8.1.3.tar.xz)をダウンロードし、固定SHA-256を照合してMinGWでクロスビルドします。GPL・nonfree・version3・外部ライブラリの自動検出を無効化し、FFmpegのライブラリは実行ファイルへ静的リンクします。アプリはFFmpegへリンクせず、独立したプロセスとして実行します。再エンコード用の外部x264/x265などは追加していません。ネットワーク入力とffplayは無効です。

同梱ZIPの `ffmpeg/` に、取得したソースアーカイブ、ライセンス原文、コンパイラ情報・configure引数、ビルドスクリプトを保存します。Animeta本体のMITライセンスとFFmpegのライセンスは別です。FFmpegのライセンス情報は[公式ページ](https://ffmpeg.org/legal.html)を参照してください。

```sh
# Ubuntu: gcc-mingw-w64-x86-64 / make / curl / xz-utilsが必要
bash scripts/build-ffmpeg-windows.sh .release-tools
python3 scripts/release.py windows \
  --exe src-tauri/target/x86_64-pc-windows-msvc/release/animeta.exe \
  --ffmpeg-dir .release-tools --dist .release
```

FFmpegのバージョン更新時は、スクリプトのバージョンとSHA-256、同梱チェックのソース名、テスト・文書を合わせて更新し、公式の署名を確認して新しいハッシュを採用してください。

## 検証範囲

- パッケージングテストで両ZIPの構成、ツール・ソース・ライセンスの存在、不正なタグ、不足したパッケージを検証。
- Windowsランナーで両ZIPを展開してアプリが起動を維持することを確認。同梱ツールの起動、短いMP4の生成と解析も確認。
- LinuxランナーでRust・フロントエンドのテスト、debのFFmpeg依存関係、仮想画面でアプリ起動と隔離DB作成を確認。
- Release公開前に全配布物のSHA-256を再照合。
- 署名・公証、実ユーザーのAnnict接続、全UI操作と動画処理、各Linuxディストリビューションでの実機確認はこのワークフローでは行いません。

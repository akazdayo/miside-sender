# miside-sender

マイク音声をMistral Voxtral Mini Realtimeで逐次文字起こしし、VRChatのMiSide字幕へOSC送信します。録音・音声変換・WebSocket通信・OSC送信はすべてRustで実装しています。Pythonやpw-recordは不要です。

```sh
nix develop -c cargo run -- --transcribe
# デバイス名を確認（APIキー不要）
nix develop -c cargo run -- --list-devices
# 表示された名前を指定する
nix develop -c cargo run -- --transcribe --device 'デバイス名' --osc-host 192.168.1.10
# 従来の文字列送信
nix develop -c cargo run -- 'こんにちは'
```

認証は `MISTRAL_API_KEY` 環境変数、または起動ディレクトリの `.mistral-api-key` から読み込みます（環境変数を優先）。キーファイルはGit管理対象外です。

既定モデルは `voxtral-mini-transcribe-realtime-2602`。`--model` で変更できます。CPALで既定の入力デバイスを録音し、複数チャンネルをモノラルへ混合、sincリサンプリングで16kHzへ変換して16bit little-endian PCMをMistral公式WebSocket APIへ継続送信します。無音中も送信します。Ctrl+Cで終了し、録音・通信・OSC送信を停止します。

`--device` は `--list-devices` に表示される名前との完全一致です。以前のPipeWireノード名やserialの指定方式から変更されています。

VRChatのOSCを有効にし、MiSide字幕対応アバターを使用してください。送信先は既定で `127.0.0.1:9000`（`--osc-host` / `--osc-port` で変更可能）。認識した差分文字列を順次送信します。OSCは既存のCodeLo / CodeHi / CodeTopとRevisionを使い、1文字約140msのため長文では字幕表示が遅れます。直接文字列送信時の間隔は従来どおり約210msです。

APIエラー・切断・録音エラー時は停止してエラーを表示します。音声待機キューは20チャンク（約2秒）、字幕待機キューは256差分までで、上限に達した場合もエラーで停止します。

## ビルド・配布

```sh
nix develop -c cargo build --release
# ビルドしたバイナリを直接実行
./target/release/miside-sender --transcribe
```

配布先でRustやPythonをインストールする必要はありません。対象OS・アーキテクチャごとにビルドした実行ファイルと、利用者自身のAPIキーを用意してください。キーはバイナリに埋め込まれません。NixのCargo設定等で出力先を変更している場合、ビルドログに表示される場所を参照してください。

LinuxではシステムのALSAライブラリ（libasound）と利用可能な入力デバイスが必要です。ビルド時にはALSA開発用ファイルとpkg-configも必要で、Nix devShellに含まれます。NixでビルドしたバイナリはNixストアへの依存を持つため、そのまま一般的なLinuxへコピーする配布には適しません。汎用Linux向けは対応するLinuxビルド環境でビルドしてください。Windows/macOSではCPALの各OS標準音声バックエンドを使います（このリポジトリでの実行検証はLinuxのみ）。TLSにはrustlsとOSの信頼済み証明書を使用します。

API仕様: https://docs.mistral.ai/studio/audio/speech_to_text/realtime_transcription

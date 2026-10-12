# miside-sender

PipeWireマイク音声をMistral Voxtral Mini Realtimeで逐次文字起こしし、VRChatのMiSide字幕へOSC送信します。

```sh
nix develop -c cargo run -- --transcribe
# 入力デバイス・OSC送信先の変更
nix develop -c cargo run -- --transcribe --device INPUT_NODE --osc-host 192.168.1.10
# 従来の文字列送信
nix develop -c cargo run -- 'こんにちは'
```

認証は `MISTRAL_API_KEY` 環境変数、または起動ディレクトリの `.mistral-api-key` から読み込みます（環境変数を優先）。キーファイルはGit管理対象外です。リポジトリのディレクトリから実行してください。

既定モデルは `voxtral-mini-transcribe-realtime-2602`。`--model` で変更できます。Mistral公式WebSocket APIへ16kHz・16bit・モノラル音声を継続送信します。録音中の音声はMistralへ送られます。Ctrl+Cで終了します。Pythonのwebsocketsとpw-recordはdevShellに含まれます。

VRChatのOSCを有効にし、MiSide字幕対応アバターを使用してください。送信先は既定で `127.0.0.1:9000`（`--osc-host` / `--osc-port` で変更可能）。認識した差分文字列を順次送信します。OSCは既存のCodeLo / CodeHi / CodeTopとRevisionを使い、1文字約140msのため、長文では字幕表示が遅れます。

APIエラー・切断・録音終了時は停止し、エラーを表示します。

API仕様: https://docs.mistral.ai/studio/audio/speech_to_text/realtime_transcription

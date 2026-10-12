use anyhow::{Context, Result, bail};
use base64::{Engine, engine::general_purpose::STANDARD};
use futures_util::{SinkExt, StreamExt};
use serde_json::{Value, json};
use std::{
    io::{self, Write},
    time::Duration,
};
use tokio::{net::TcpStream, sync::mpsc, time::timeout};
use tokio_tungstenite::{
    MaybeTlsStream, WebSocketStream, connect_async,
    tungstenite::{Message, client::IntoClientRequest},
};

use crate::{Args, audio::Microphone, osc};

type Connection = WebSocketStream<MaybeTlsStream<TcpStream>>;

async fn connect(model: &str, key: &str) -> Result<Connection> {
    let _ = rustls::crypto::ring::default_provider().install_default();
    let mut url = url::Url::parse("wss://api.mistral.ai/v1/audio/transcriptions/realtime")?;
    url.query_pairs_mut().append_pair("model", model);
    let mut request = url.as_str().into_client_request()?;
    let mut authorization: tokio_tungstenite::tungstenite::http::HeaderValue =
        format!("Bearer {key}")
            .parse()
            .context("APIキーに不正な文字が含まれています")?;
    // Prevent accidental header disclosure in Debug output.
    authorization.set_sensitive(true);
    request.headers_mut().insert("Authorization", authorization);
    let (mut ws, _) = timeout(Duration::from_secs(20), connect_async(request))
        .await
        .context("API接続がタイムアウトしました")??;
    timeout(Duration::from_secs(20), async {
        loop {
            let event = receive_event(&mut ws).await?;
            if event["type"] == "session.created" {
                return Ok::<_, anyhow::Error>(());
            }
        }
    })
    .await
    .context("APIセッション作成がタイムアウトしました")??;
    ws.send(Message::Text(
        json!({"type": "session.update", "session": {
            "audio_format": {"encoding": "pcm_s16le", "sample_rate": 16000},
        }})
        .to_string()
        .into(),
    ))
    .await?;
    Ok(ws)
}

async fn receive_event(ws: &mut Connection) -> Result<Value> {
    loop {
        let message = ws
            .next()
            .await
            .context("文字起こしAPIとの接続が切断されました")??;
        match message {
            Message::Text(text) => return parse_event(text.as_str()),
            Message::Binary(bytes) => return parse_event(std::str::from_utf8(&bytes)?),
            Message::Ping(data) => ws.send(Message::Pong(data)).await?,
            Message::Close(_) => bail!("文字起こしAPIとの接続が終了しました"),
            _ => {}
        }
    }
}

fn parse_event(text: &str) -> Result<Value> {
    let event: Value = serde_json::from_str(text).context("API応答が不正なJSONです")?;
    if event["type"] == "error" {
        bail!("文字起こしAPIエラー: {}", event["error"]);
    }
    Ok(event)
}

pub async fn run(args: &Args, key: &str) -> Result<()> {
    let mut microphone = Microphone::open(args.device.as_deref())?;
    let mut sender = osc::Sender::connect(&args.osc_host, args.osc_port).await?;
    let mut ws = connect(&args.model, key).await?;
    microphone.start()?;
    println!(
        "マイク文字起こし開始: {} → OSC {}:{} (Ctrl+Cで終了)",
        args.model, args.osc_host, args.osc_port
    );
    let (pending, texts) = mpsc::channel(256);
    let session = async move {
        loop {
            tokio::select! {
                changed = microphone.errors.changed() => {
                    changed.context("録音エラー通知が終了しました")?;
                    bail!("{}", microphone.errors.borrow().as_deref().unwrap_or("録音が停止しました"));
                }
                samples = microphone.frames.recv() => {
                    let pcm = microphone.converter.convert(samples.context("マイク録音が終了しました")?)?;
                    timeout(Duration::from_secs(10), ws.send(Message::Text(json!({
                        "type": "input_audio.append", "audio": STANDARD.encode(pcm),
                    }).to_string().into()))).await.context("音声送信がタイムアウトしました")??;
                }
                event = receive_event(&mut ws) => {
                    let event = event?;
                    match event["type"].as_str() {
                        Some("transcription.text.delta") => {
                            let text = event["text"].as_str().context("文字起こし差分にtextがありません")?;
                            if !text.is_empty() {
                                print!("{text}");
                                io::stdout().flush()?;
                                pending.try_send(text.to_owned()).context("字幕送信が追いつかないため停止しました")?;
                            }
                        }
                        Some("transcription.done") => return Ok::<_, anyhow::Error>(()),
                        _ => {}
                    }
                }
            }
        }
    };
    tokio::try_join!(session, sender.send_pending(texts))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn handles_delta_done_unknown_and_api_errors() {
        let event = parse_event(r#"{"type":"transcription.text.delta","text":"猫😀"}"#).unwrap();
        assert_eq!(event["text"], "猫😀");
        assert!(parse_event(r#"{"type":"transcription.done"}"#).is_ok());
        assert!(parse_event(r#"{"type":"future.event"}"#).is_ok());
        assert!(
            parse_event(r#"{"type":"error","error":{"message":"quota exceeded"}}"#)
                .unwrap_err()
                .to_string()
                .contains("quota exceeded")
        );
        assert!(parse_event("invalid json").is_err());
    }
}

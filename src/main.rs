mod audio;
mod osc;
mod transcribe;

use anyhow::{Context, Result};
use clap::Parser;
use std::time::Duration;

#[derive(Parser)]
#[command(about = "Mistral Voxtral Mini Realtime → MiSide OSC")]
pub struct Args {
    /// マイク音声を逐次文字起こしする
    #[arg(long, conflicts_with_all = ["text", "list_devices"])]
    transcribe: bool,
    /// 入力デバイス名を列挙する（APIキー不要）
    #[arg(long, conflicts_with = "text")]
    list_devices: bool,
    /// --list-devicesで表示される入力デバイス名
    #[arg(long)]
    device: Option<String>,
    #[arg(long, default_value = "voxtral-mini-transcribe-realtime-3-0")]
    model: String,
    #[arg(long, default_value = "127.0.0.1")]
    osc_host: String,
    #[arg(long, default_value_t = 9000)]
    osc_port: u16,
    /// 直接OSC送信する文字列（既定: 猫）
    text: Option<String>,
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<()> {
    let args = Args::parse();
    if args.list_devices {
        return audio::list_devices();
    }
    if args.transcribe {
        let key = load_api_key()?;
        tokio::select! {
            result = transcribe::run(&args, &key) => result,
            signal = tokio::signal::ctrl_c() => {
                signal.context("終了シグナルを受信できません")?;
                println!();
                Ok(())
            }
        }
    } else {
        let mut sender = osc::Sender::connect(&args.osc_host, args.osc_port).await?;
        sender
            .send_text(
                args.text.as_deref().unwrap_or("猫"),
                Duration::from_millis(300),
            )
            .await?;
        println!(
            "{}:{} へ OSC メッセージを送信しました。",
            args.osc_host, args.osc_port
        );
        Ok(())
    }
}

fn load_api_key() -> Result<String> {
    let key = std::env::var("MISTRAL_API_KEY").unwrap_or_default();
    let key = if key.trim().is_empty() {
        std::fs::read_to_string(".mistral-api-key")
            .context("MISTRAL_API_KEY または .mistral-api-key を設定してください")?
    } else {
        key
    };
    anyhow::ensure!(!key.trim().is_empty(), "APIキーが空です");
    Ok(key.trim().to_owned())
}

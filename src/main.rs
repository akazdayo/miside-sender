use anyhow::Result;
use rosc::{OscBundle, OscMessage, OscPacket, OscType};
use rosc::{OscTime, encoder};
use std::net::UdpSocket;
use std::time::UNIX_EPOCH;
use thiserror::Error;

fn main() -> Result<()> {
    let socket = UdpSocket::bind("127.0.0.1:0").expect("Cannot bind local socket");
    let target_addr = "127.0.0.1:8125";

    let msg = convert_message("猫".as_bytes())?;
    // パケット（メッセージ）をバイト配列にエンコード
    let packet = OscPacket::Bundle(OscBundle {
        timetag: OscTime::try_from(UNIX_EPOCH)?,
        content: msg,
    });
    let msg_buf = encoder::encode(&packet).expect("Cannot encode OSC packet");

    // UDP で送信
    socket
        .send_to(&msg_buf, target_addr)
        .expect("Cannot send data");
    println!("{} へ OSC メッセージを送信しました。", target_addr);
    Ok(())
}

#[derive(Error, Debug)]
pub enum ConvertMessageError {
    #[error("Invalid input")]
    InvalidInput(),
}

fn convert_message(data: &[u8]) -> Result<Vec<OscPacket>, ConvertMessageError> {
    let all_addr = [
        "/avatar/parameters/bytes0",
        "/avatar/parameters/bytes1",
        "/avatar/parameters/bytes2",
        "/avatar/parameters/bytes3",
    ];

    if data.len() >= 5 {
        return Err(ConvertMessageError::InvalidInput());
    }

    let messages = data
        .iter()
        .enumerate()
        .map(|(i, x)| {
            OscPacket::Message(OscMessage {
                addr: all_addr[i].to_string(),
                args: vec![OscType::Int(x.clone() as i32)],
            })
        })
        .collect::<Vec<_>>();
    Ok(messages)
}

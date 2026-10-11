use anyhow::Result;
use rosc::{OscBundle, OscMessage, OscPacket, OscType};
use rosc::{OscTime, encoder};
use std::net::UdpSocket;
use std::time::UNIX_EPOCH;

fn main() -> Result<()> {
    let socket = UdpSocket::bind("127.0.0.1:0").expect("Cannot bind local socket");
    let target_addr = "127.0.0.1:8125";

    let msg = convert_message(&to_fixed4('猫'))?;
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

fn to_fixed4(c: char) -> [u8; 4] {
    let mut out = [0u8; 4];
    c.encode_utf8(&mut out);
    out
}

fn convert_message(data: &[u8; 4]) -> Result<Vec<OscPacket>> {
    let all_addr = [
        "/avatar/parameters/bytes0",
        "/avatar/parameters/bytes1",
        "/avatar/parameters/bytes2",
        "/avatar/parameters/bytes3",
    ];

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

#[cfg(test)]
mod tests {
    use super::to_fixed4;

    #[test]
    fn encodes_a_char_as_zero_padded_utf8() {
        let encode: fn(char) -> [u8; 4] = to_fixed4;
        for (c, expected) in [
            ('A', [0x41, 0, 0, 0]),
            ('é', [0xC3, 0xA9, 0, 0]),
            ('猫', [0xE7, 0x8C, 0xAB, 0]),
            ('😀', [0xF0, 0x9F, 0x98, 0x80]),
            ('\0', [0, 0, 0, 0]),
            ('\u{10FFFF}', [0xF4, 0x8F, 0xBF, 0xBF]),
        ] {
            assert_eq!(encode(c), expected, "incorrect encoding for {c:?}");
        }
    }
}

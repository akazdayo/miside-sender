use anyhow::Result;
use rosc::encoder;
use rosc::{OscBundle, OscMessage, OscPacket, OscType};
use std::net::UdpSocket;
use std::time::Duration;

fn main() -> Result<()> {
    let socket = UdpSocket::bind("127.0.0.1:0").expect("Cannot bind local socket");
    let target_addr = "127.0.0.1:9000";

    let text = std::env::args().nth(1).unwrap_or_else(|| "猫".to_string());
    let mut revision = 0u8;
    for c in text.chars() {
        let msg = convert_message(&to_fixed3(c))?;
        // パケット（メッセージ）をバイト配列にエンコード
        let packet = OscPacket::Bundle(OscBundle {
            timetag: (0, 1).into(), // 即時実行
            content: msg,
        });
        let msg_buf = encoder::encode(&packet).expect("Cannot encode OSC packet");

        // UDP で送信
        socket
            .send_to(&msg_buf, target_addr)
            .expect("Cannot send data");
        // 文字データが反映されてから更新番号を送る。同じ文字の連続入力も検出するため。
        std::thread::sleep(Duration::from_millis(60));
        revision = revision.wrapping_add(1);
        let update = OscPacket::Message(OscMessage {
            addr: "/avatar/parameters/MiSide/Revision".to_string(),
            args: vec![OscType::Int(revision as i32)],
        });
        socket.send_to(&encoder::encode(&update)?, target_addr)?;
        std::thread::sleep(Duration::from_millis(150));
    }
    println!("{} へ OSC メッセージを送信しました。", target_addr);
    Ok(())
}

fn to_fixed3(c: char) -> [u8; 3] {
    // UTF-8ではなくUnicode文字番号を下位バイトから送る（最大21bit）。
    let bytes = (c as u32).to_le_bytes();
    [bytes[0], bytes[1], bytes[2]]
}

fn convert_message(data: &[u8; 3]) -> Result<Vec<OscPacket>> {
    let all_addr = [
        "/avatar/parameters/MiSide/CodeLo",
        "/avatar/parameters/MiSide/CodeHi",
        "/avatar/parameters/MiSide/CodeTop",
    ];

    let messages = data
        .iter()
        .enumerate()
        .map(|(i, x)| {
            OscPacket::Message(OscMessage {
                addr: all_addr[i].to_string(),
                args: vec![OscType::Int(*x as i32)],
            })
        })
        .collect::<Vec<_>>();
    Ok(messages)
}

#[cfg(test)]
mod tests {
    use super::to_fixed3;

    #[test]
    fn encodes_a_char_as_unicode_number() {
        let encode: fn(char) -> [u8; 3] = to_fixed3;
        for (c, expected) in [
            ('A', [0x41, 0, 0]),
            ('é', [0xE9, 0, 0]),
            ('猫', [0x2B, 0x73, 0]),
            ('😀', [0, 0xF6, 1]),
            ('\0', [0, 0, 0]),
            ('\u{10FFFF}', [0xFF, 0xFF, 0x10]),
        ] {
            assert_eq!(encode(c), expected, "incorrect encoding for {c:?}");
        }
    }
}

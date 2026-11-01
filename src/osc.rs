use anyhow::Result;
use rosc::{OscBundle, OscMessage, OscPacket, OscType, encoder};
use std::time::Duration;
use tokio::{net::UdpSocket, sync::mpsc, time::sleep};

pub struct Sender {
    socket: UdpSocket,
    revision: u8,
}

impl Sender {
    pub async fn connect(host: &str, port: u16) -> Result<Self> {
        let target = tokio::net::lookup_host((host, port))
            .await?
            .next()
            .ok_or_else(|| anyhow::anyhow!("OSC送信先を解決できません"))?;
        let socket = UdpSocket::bind(if target.is_ipv4() {
            "0.0.0.0:0"
        } else {
            "[::]:0"
        })
        .await?;
        socket.connect(target).await?;
        Ok(Self {
            socket,
            revision: 0,
        })
    }

    pub async fn send_text(&mut self, text: &str, interval: Duration) -> Result<()> {
        for character in text.chars() {
            self.socket
                .send(&encoder::encode(&character_packet(character))?)
                .await?;
            sleep(Duration::from_millis(60)).await;
            self.revision = (self.revision + 1) % 8;
            self.socket
                .send(&encoder::encode(&OscPacket::Message(OscMessage {
                    addr: "/avatar/parameters/MiSide/CodeTop".into(),
                    args: vec![OscType::Int(
                        ((self.revision as u32) << 5 | ((character as u32) >> 16)) as i32,
                    )],
                }))?)
                .await?;
            sleep(interval).await;
        }
        Ok(())
    }

    pub async fn send_pending(&mut self, mut pending: mpsc::Receiver<String>) -> Result<()> {
        while let Some(text) = pending.recv().await {
            self.send_text(&text, Duration::from_millis(80)).await?;
        }
        Ok(())
    }
}

fn to_fixed3(c: char) -> [u8; 3] {
    let bytes = (c as u32).to_le_bytes();
    [bytes[0], bytes[1], bytes[2]]
}

fn character_packet(character: char) -> OscPacket {
    let content = ["CodeLo", "CodeHi"]
        .into_iter()
        .zip(to_fixed3(character))
        .map(|(name, value)| {
            OscPacket::Message(OscMessage {
                addr: format!("/avatar/parameters/MiSide/{name}"),
                args: vec![OscType::Int(value.into())],
            })
        })
        .collect();
    OscPacket::Bundle(OscBundle {
        timetag: (0, 1).into(),
        content,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encodes_unicode_number() {
        for (c, expected) in [
            ('A', [65, 0, 0]),
            ('é', [233, 0, 0]),
            ('猫', [43, 115, 0]),
            ('😀', [0, 246, 1]),
            ('\0', [0, 0, 0]),
            ('\u{10ffff}', [255, 255, 16]),
        ] {
            assert_eq!(to_fixed3(c), expected);
        }
    }

    #[tokio::test]
    async fn loopback_preserves_repeated_chars_and_revision_wrap() {
        let receiver = UdpSocket::bind("127.0.0.1:0").await.unwrap();
        let mut sender = Sender::connect("127.0.0.1", receiver.local_addr().unwrap().port())
            .await
            .unwrap();
        sender.revision = 6;
        sender.send_text("猫猫😀", Duration::ZERO).await.unwrap();
        let mut buffer = [0; 1024];
        for (character, revision) in "猫猫😀".chars().zip([7, 0, 1]) {
            for expected in [
                character_packet(character),
                OscPacket::Message(OscMessage {
                    addr: "/avatar/parameters/MiSide/CodeTop".into(),
                    args: vec![OscType::Int((revision << 5) | ((character as i32) >> 16))],
                }),
            ] {
                let length =
                    tokio::time::timeout(Duration::from_secs(1), receiver.recv(&mut buffer))
                        .await
                        .unwrap()
                        .unwrap();
                assert_eq!(
                    rosc::decoder::decode_udp(&buffer[..length]).unwrap().1,
                    expected
                );
            }
        }
    }
}

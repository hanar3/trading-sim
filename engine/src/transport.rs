//! Bounded, length-prefixed command decoding. A bad frame closes its connection.
use crate::messages::trading::{WireMessage, wire_message::Payload};
use prost::Message;
use std::io;
use tokio::io::{AsyncRead, AsyncReadExt};

pub const MAX_FRAME_BYTES: usize = 4096;

pub async fn read_command(reader: &mut (impl AsyncRead + Unpin)) -> io::Result<Payload> {
    let length = reader.read_u32().await? as usize;
    if length == 0 || length > MAX_FRAME_BYTES {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "invalid frame size",
        ));
    }
    let mut bytes = vec![0; length];
    reader.read_exact(&mut bytes).await?;
    let message = WireMessage::decode(bytes.as_slice())
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
    match message.payload {
        Some(command @ (Payload::PlaceLimitOrder(_) | Payload::CancelOrder(_))) => Ok(command),
        _ => Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "expected command payload",
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::messages::trading::CancelOrder;

    #[tokio::test]
    async fn rejects_unbounded_empty_malformed_missing_and_truncated_frames() {
        for bytes in [
            u32::MAX.to_be_bytes().to_vec(),
            0u32.to_be_bytes().to_vec(),
            vec![0, 0, 0, 1, 255],
            vec![0, 0, 0, 2, 8, 1], // valid protobuf without a command
            vec![0, 0, 0, 2, 1],
        ] {
            assert!(read_command(&mut bytes.as_slice()).await.is_err());
        }
    }

    #[tokio::test]
    async fn accepts_fragmented_and_consecutive_frames() {
        use tokio::io::AsyncWriteExt;
        let (mut writer, mut reader) = tokio::io::duplex(8);
        let task = tokio::spawn(async move {
            for id in [17, 18] {
                let bytes = WireMessage {
                    payload: Some(Payload::CancelOrder(CancelOrder { order_id: id })),
                }
                .encode_to_vec();
                writer.write_u32(bytes.len() as u32).await.unwrap();
                for byte in bytes {
                    writer.write_all(&[byte]).await.unwrap();
                }
            }
        });
        for expected in [17, 18] {
            let Payload::CancelOrder(order) = read_command(&mut reader).await.unwrap() else {
                panic!("wrong payload")
            };
            assert_eq!(order.order_id, expected);
        }
        task.await.unwrap();
    }
}

//! 与 TypeScript CBOR transport 互操作的最小编解码边界。
//!
//! 本模块只负责将已通过协议 DTO 类型约束的数据编码为 CBOR，或将 CBOR 解码回目标 DTO。
//! 线协议的长度前缀由 [`crate::framing`] 负责；调用方仍应在连接边界使用具体的
//! [`crate::ClientMessage`] 或 [`crate::ServerMessage`] 类型，而不是直接传输任意 JSON 值。

use std::fmt;
use std::io::Cursor;

use serde::de::DeserializeOwned;
use serde::Serialize;

/// CBOR 编码或解码失败时的稳定错误类型。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CborError {
    /// Rust DTO 无法编码成 CBOR。
    Encode(String),
    /// CBOR 字节不合法，或不能反序列化成目标 DTO。
    Decode(String),
    /// 输入在第一个 CBOR 项之后还有额外字节。
    TrailingBytes,
}

impl fmt::Display for CborError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Encode(message) => write!(formatter, "failed to encode CBOR: {message}"),
            Self::Decode(message) => write!(formatter, "failed to decode CBOR: {message}"),
            Self::TrailingBytes => formatter.write_str("CBOR payload contains trailing bytes"),
        }
    }
}

impl std::error::Error for CborError {}

/// 将一个已经受 Rust 协议 DTO 约束的值编码为 RFC 8949 CBOR。
///
/// 该函数不接受网络长度限制；发送前应使用 [`crate::framing::encode_frame`] 施加帧上限。
pub fn encode_cbor<T: Serialize>(value: &T) -> Result<Vec<u8>, CborError> {
    let mut bytes = Vec::new();
    ciborium::ser::into_writer(value, &mut bytes)
        .map_err(|error| CborError::Encode(error.to_string()))?;
    Ok(bytes)
}

/// 将恰好一个 CBOR 项解码为指定的 Rust 协议 DTO。
///
/// 额外字节通常意味着帧边界损坏或调用方把多个 CBOR 项拼入同一个 frame，必须拒绝。
pub fn decode_cbor<T: DeserializeOwned>(bytes: &[u8]) -> Result<T, CborError> {
    let mut cursor = Cursor::new(bytes);
    let value = ciborium::de::from_reader(&mut cursor)
        .map_err(|error| CborError::Decode(error.to_string()))?;
    if cursor.position() != bytes.len() as u64 {
        return Err(CborError::TrailingBytes);
    }
    Ok(value)
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::{decode_cbor, encode_cbor, CborError};
    use crate::{ClientMessage, Command};

    #[test]
    fn round_trips_a_typed_client_message() {
        let message = ClientMessage::Request {
            id: "request-1".to_owned(),
            request: Command::Abort {
                session_id: "session-1".to_owned(),
            },
        };

        let encoded = encode_cbor(&message).expect("message should encode");
        assert_eq!(decode_cbor::<ClientMessage>(&encoded), Ok(message));
    }

    #[test]
    fn produces_the_same_definite_length_wire_shape_as_typescript_for_a_request() {
        let message = ClientMessage::Request {
            id: "request-1".to_owned(),
            request: Command::Abort {
                session_id: "session-1".to_owned(),
            },
        };

        // 此 fixture 对应 TypeScript `encodeCbor` 对同一 JSON 对象的输出：
        // { type: "request", id: "request-1", request: { command: "abort", sessionId: "session-1" } }
        assert_eq!(
            encode_cbor(&message).expect("message should encode"),
            vec![
                0xa3, 0x64, b't', b'y', b'p', b'e', 0x67, b'r', b'e', b'q', b'u', b'e', b's', b't',
                0x62, b'i', b'd', 0x69, b'r', b'e', b'q', b'u', b'e', b's', b't', b'-', b'1', 0x67,
                b'r', b'e', b'q', b'u', b'e', b's', b't', 0xa2, 0x67, b'c', b'o', b'm', b'm', b'a',
                b'n', b'd', 0x65, b'a', b'b', b'o', b'r', b't', 0x69, b's', b'e', b's', b's', b'i',
                b'o', b'n', b'I', b'd', 0x69, b's', b'e', b's', b's', b'i', b'o', b'n', b'-', b'1',
            ]
        );
    }

    #[test]
    fn rejects_trailing_cbor_items_and_invalid_typed_payloads() {
        let mut encoded = encode_cbor(&json!({ "type": "hello", "version": 1 }))
            .expect("first item should encode");
        encoded.extend_from_slice(&encode_cbor(&json!(null)).expect("second item should encode"));
        assert_eq!(
            decode_cbor::<ClientMessage>(&encoded),
            Err(CborError::TrailingBytes)
        );

        let invalid = encode_cbor(&json!({ "type": "hello", "version": 1, "extra": true }))
            .expect("invalid JSON value can still be valid CBOR");
        assert!(matches!(
            decode_cbor::<ClientMessage>(&invalid),
            Err(CborError::Decode(_))
        ));
    }
}

//! 与 TypeScript `framing.ts` 对齐的长度前缀帧格式。
//!
//! 每帧由 4 字节无符号大端 payload 长度和一个 CBOR payload 组成。该模块只处理
//! 字节边界和长度限制，不理解 payload 的业务结构，避免 Transport 依赖 Agent 模型。

use std::fmt;

/// TypeScript 协议使用的默认单帧最大 payload 长度：16 MiB。
pub const DEFAULT_MAX_FRAME_LENGTH: usize = 16 * 1024 * 1024;
const FRAME_HEADER_LENGTH: usize = 4;

/// 帧格式或长度限制不合法时返回的错误。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FrameError {
    /// 调用方提供的最大帧长度无法表示为协议的 u32 长度字段。
    InvalidMaximumLength(usize),
    /// 编码 payload 超过了 u32 或配置的长度上限。
    PayloadTooLarge { length: usize, maximum: usize },
    /// 输入未包含完整的四字节长度前缀。
    IncompleteHeader,
    /// 输入没有恰好包含一个完整帧。
    IncompletePayload { expected: usize, actual: usize },
    /// 在读取到完整帧前结束了字节流。
    TruncatedStream,
    /// 已结束的 decoder 不允许再接收字节。
    DecoderEnded,
    /// 已失败的 decoder 不允许继续使用。
    DecoderFailed,
}

impl fmt::Display for FrameError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidMaximumLength(length) => write!(
                formatter,
                "maximum frame length {length} exceeds the unsigned 32-bit protocol limit"
            ),
            Self::PayloadTooLarge { length, maximum } => {
                write!(
                    formatter,
                    "frame length {length} exceeds configured limit of {maximum}"
                )
            }
            Self::IncompleteHeader => {
                formatter.write_str("frame does not contain a complete length prefix")
            }
            Self::IncompletePayload { expected, actual } => write!(
                formatter,
                "frame must contain exactly one complete payload: expected {expected} bytes, got {actual}"
            ),
            Self::TruncatedStream => formatter.write_str("truncated frame at end of stream"),
            Self::DecoderEnded => formatter.write_str("frame decoder has ended"),
            Self::DecoderFailed => formatter.write_str("frame decoder has failed"),
        }
    }
}

impl std::error::Error for FrameError {}

/// 以 4 字节大端长度前缀封装一个 payload。
pub fn encode_frame(payload: &[u8], max_frame_length: usize) -> Result<Vec<u8>, FrameError> {
    validate_maximum_length(max_frame_length)?;
    if payload.len() > max_frame_length {
        return Err(FrameError::PayloadTooLarge {
            length: payload.len(),
            maximum: max_frame_length,
        });
    }

    let length = u32::try_from(payload.len()).map_err(|_| FrameError::PayloadTooLarge {
        length: payload.len(),
        maximum: u32::MAX as usize,
    })?;
    let mut frame = Vec::with_capacity(FRAME_HEADER_LENGTH + payload.len());
    frame.extend_from_slice(&length.to_be_bytes());
    frame.extend_from_slice(payload);
    Ok(frame)
}

/// 验证字节恰好是一帧并返回 payload 切片。
pub fn decode_complete_frame(frame: &[u8], max_frame_length: usize) -> Result<&[u8], FrameError> {
    validate_maximum_length(max_frame_length)?;
    if frame.len() < FRAME_HEADER_LENGTH {
        return Err(FrameError::IncompleteHeader);
    }

    let declared_length = u32::from_be_bytes([frame[0], frame[1], frame[2], frame[3]]) as usize;
    if declared_length > max_frame_length {
        return Err(FrameError::PayloadTooLarge {
            length: declared_length,
            maximum: max_frame_length,
        });
    }

    let actual_length = frame.len() - FRAME_HEADER_LENGTH;
    if actual_length != declared_length {
        return Err(FrameError::IncompletePayload {
            expected: declared_length,
            actual: actual_length,
        });
    }
    Ok(&frame[FRAME_HEADER_LENGTH..])
}

/// 逐块读取传输字节并产出完整 payload。
///
/// 该状态机与 TypeScript [`FrameDecoder`](../../../../packages/protocol/src/framing.ts)
/// 具有相同的边界语义：一个 chunk 可以包含不完整帧、完整帧或多个连续帧。输出的
/// `Vec<u8>` 独立拥有数据，因此调用方可在下一次 [`FrameDecoder::push`] 前安全保留它。
#[derive(Debug, Clone)]
pub struct FrameDecoder {
    max_frame_length: usize,
    header: [u8; FRAME_HEADER_LENGTH],
    header_length: usize,
    expected_payload_length: Option<usize>,
    payload: Vec<u8>,
    state: DecoderState,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DecoderState {
    Open,
    Ended,
    Failed,
}

impl FrameDecoder {
    /// 使用 TypeScript 协议默认的 16 MiB 单帧上限创建 decoder。
    pub fn new() -> Self {
        Self::with_max_frame_length(DEFAULT_MAX_FRAME_LENGTH)
            .expect("the built-in frame length limit must be valid")
    }

    /// 使用调用方指定的单帧 payload 上限创建 decoder。
    pub fn with_max_frame_length(max_frame_length: usize) -> Result<Self, FrameError> {
        validate_maximum_length(max_frame_length)?;
        Ok(Self {
            max_frame_length,
            header: [0; FRAME_HEADER_LENGTH],
            header_length: 0,
            expected_payload_length: None,
            payload: Vec::new(),
            state: DecoderState::Open,
        })
    }

    /// 接收任意长度的字节块，并返回本次完整解出的全部 payload。
    pub fn push(&mut self, chunk: &[u8]) -> Result<Vec<Vec<u8>>, FrameError> {
        match self.state {
            DecoderState::Ended => return Err(FrameError::DecoderEnded),
            DecoderState::Failed => return Err(FrameError::DecoderFailed),
            DecoderState::Open => {}
        }

        let mut frames = Vec::new();
        let mut offset = 0;
        while offset < chunk.len() {
            if self.expected_payload_length.is_none() {
                let header_bytes =
                    (FRAME_HEADER_LENGTH - self.header_length).min(chunk.len() - offset);
                self.header[self.header_length..self.header_length + header_bytes]
                    .copy_from_slice(&chunk[offset..offset + header_bytes]);
                self.header_length += header_bytes;
                offset += header_bytes;
                if self.header_length < FRAME_HEADER_LENGTH {
                    continue;
                }

                let expected_payload_length = u32::from_be_bytes(self.header) as usize;
                self.header_length = 0;
                if expected_payload_length > self.max_frame_length {
                    return Err(self.fail(FrameError::PayloadTooLarge {
                        length: expected_payload_length,
                        maximum: self.max_frame_length,
                    }));
                }
                if expected_payload_length == 0 {
                    frames.push(Vec::new());
                    continue;
                }

                self.expected_payload_length = Some(expected_payload_length);
                self.payload = Vec::with_capacity(expected_payload_length);
            }

            let expected_payload_length = self
                .expected_payload_length
                .expect("a payload length is set after a complete nonempty header");
            let payload_bytes =
                (expected_payload_length - self.payload.len()).min(chunk.len() - offset);
            self.payload
                .extend_from_slice(&chunk[offset..offset + payload_bytes]);
            offset += payload_bytes;

            if self.payload.len() == expected_payload_length {
                frames.push(std::mem::take(&mut self.payload));
                self.expected_payload_length = None;
            }
        }
        Ok(frames)
    }

    /// 标记输入结束。若仍有未完成 header 或 payload，则 decoder 进入失败状态。
    pub fn end(&mut self) -> Result<(), FrameError> {
        match self.state {
            DecoderState::Ended => return Err(FrameError::DecoderEnded),
            DecoderState::Failed => return Err(FrameError::DecoderFailed),
            DecoderState::Open => {}
        }
        if self.header_length != 0 || self.expected_payload_length.is_some() {
            return Err(self.fail(FrameError::TruncatedStream));
        }
        self.state = DecoderState::Ended;
        Ok(())
    }

    fn fail(&mut self, error: FrameError) -> FrameError {
        self.state = DecoderState::Failed;
        self.header_length = 0;
        self.expected_payload_length = None;
        self.payload.clear();
        error
    }
}

impl Default for FrameDecoder {
    fn default() -> Self {
        Self::new()
    }
}

fn validate_maximum_length(max_frame_length: usize) -> Result<(), FrameError> {
    if max_frame_length > u32::MAX as usize {
        return Err(FrameError::InvalidMaximumLength(max_frame_length));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{
        DEFAULT_MAX_FRAME_LENGTH, FrameDecoder, FrameError, decode_complete_frame, encode_frame,
    };

    #[test]
    fn round_trips_a_big_endian_frame() {
        let frame =
            encode_frame(&[1, 2, 3], DEFAULT_MAX_FRAME_LENGTH).expect("frame should encode");
        assert_eq!(frame, [0, 0, 0, 3, 1, 2, 3]);
        assert_eq!(
            decode_complete_frame(&frame, DEFAULT_MAX_FRAME_LENGTH),
            Ok(&[1, 2, 3][..])
        );
    }

    #[test]
    fn rejects_incomplete_and_oversized_frames() {
        assert_eq!(
            decode_complete_frame(&[0, 0, 0], DEFAULT_MAX_FRAME_LENGTH),
            Err(FrameError::IncompleteHeader)
        );
        assert_eq!(
            decode_complete_frame(&[0, 0, 0, 2, 1], DEFAULT_MAX_FRAME_LENGTH),
            Err(FrameError::IncompletePayload {
                expected: 2,
                actual: 1
            })
        );
        assert_eq!(
            encode_frame(&[1, 2], 1),
            Err(FrameError::PayloadTooLarge {
                length: 2,
                maximum: 1
            })
        );
    }

    #[test]
    fn incrementally_decodes_fragmented_and_coalesced_frames() {
        let first =
            encode_frame(&[1, 2], DEFAULT_MAX_FRAME_LENGTH).expect("first frame should encode");
        let second =
            encode_frame(&[3], DEFAULT_MAX_FRAME_LENGTH).expect("second frame should encode");
        let mut decoder = FrameDecoder::new();

        assert_eq!(decoder.push(&first[..3]), Ok(Vec::new()));
        let mut chunk = first[3..].to_vec();
        chunk.extend_from_slice(&second);
        assert_eq!(decoder.push(&chunk), Ok(vec![vec![1, 2], vec![3]]));
        assert_eq!(decoder.end(), Ok(()));
        assert_eq!(decoder.push(&[]), Err(FrameError::DecoderEnded));
    }

    #[test]
    fn decoder_rejects_truncated_and_oversized_streams_permanently() {
        let mut truncated = FrameDecoder::new();
        assert_eq!(truncated.push(&[0, 0, 0]), Ok(Vec::new()));
        assert_eq!(truncated.end(), Err(FrameError::TruncatedStream));
        assert_eq!(truncated.end(), Err(FrameError::DecoderFailed));

        let mut oversized = FrameDecoder::with_max_frame_length(1).expect("limit should be valid");
        assert_eq!(
            oversized.push(&[0, 0, 0, 2]),
            Err(FrameError::PayloadTooLarge {
                length: 2,
                maximum: 1
            })
        );
        assert_eq!(oversized.push(&[]), Err(FrameError::DecoderFailed));
    }
}

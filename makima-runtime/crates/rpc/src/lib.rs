//! Rust Core 的传输中立 RPC 连接状态机。
//!
//! 本 crate 只处理共享协议的 CBOR 帧、版本协商和消息顺序。业务命令、会话持久化及
//! Provider 调用均通过 [`RpcCommandHandler`] 注入，避免 RPC 层反向依赖 Runtime 或
//! TypeScript Host 的实现。

use std::{
    fmt,
    io::{self, Read, Write},
};

use protocol::{
    ClientMessage, Command, CommandResult, ErrorResponse, EventEnvelope, EventMessageType,
    HelloErrorMessageType, HelloMessageType, PROTOCOL_VERSION, ProtocolError, ProtocolErrorCode,
    ServerEvent, ServerHello, ServerHelloError, ServerMessage, ServerSnapshot, SuccessResponse,
    cbor::{CborError, decode_cbor, encode_cbor},
    framing::{DEFAULT_MAX_FRAME_LENGTH, FrameDecoder, FrameError, encode_frame},
};

/// RPC 连接的业务端口。
///
/// 该端口刻意同步且窄：外层 stdio、socket 或 async runtime 负责调度，连接状态机只保证
/// 同一连接内的 command response 与其后事件按协议顺序输出。
pub trait RpcCommandHandler {
    /// 执行一条已完成版本协商的命令。
    fn execute(&mut self, command: Command) -> Result<CommandResult, ProtocolError>;

    /// 返回当前权威服务端快照，用于 hello 建立初始视图。
    fn snapshot(&self) -> ServerSnapshot;

    /// 取走自上次调用以来产生的事件，顺序即为发送顺序。
    fn drain_events(&mut self) -> Vec<ServerEvent>;
}

/// 可由 stdio、Unix socket 或测试内存连接驱动的 RPC connection。
pub struct RpcConnection<H> {
    handler: H,
    decoder: FrameDecoder,
    connection_id: String,
    stage: ConnectionStage,
    next_event_sequence: u64,
    max_frame_length: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ConnectionStage {
    AwaitingHello,
    Ready,
    Closed,
}

/// RPC 边界错误。协议可表达的业务失败使用 [`ProtocolError`] 写入 response；这里仅表示
/// 无法继续解析字节流、违反连接顺序或尝试使用已关闭连接。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RpcError {
    Closed,
    Transport(String),
    InvalidMessage(String),
}

impl fmt::Display for RpcError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Closed => formatter.write_str("RPC connection is closed"),
            Self::Transport(message) => write!(formatter, "RPC transport error: {message}"),
            Self::InvalidMessage(message) => write!(formatter, "Invalid RPC message: {message}"),
        }
    }
}

impl std::error::Error for RpcError {}

impl<H> RpcConnection<H>
where
    H: RpcCommandHandler,
{
    /// 使用默认 16 MiB 帧上限创建等待 hello 的连接。
    pub fn new(connection_id: impl Into<String>, handler: H) -> Self {
        Self::with_max_frame_length(connection_id, handler, DEFAULT_MAX_FRAME_LENGTH)
            .expect("default frame length must be valid")
    }

    /// 允许 transport 按自身约束收紧帧上限。
    pub fn with_max_frame_length(
        connection_id: impl Into<String>,
        handler: H,
        max_frame_length: usize,
    ) -> Result<Self, RpcError> {
        let decoder = FrameDecoder::with_max_frame_length(max_frame_length).map_err(frame_error)?;
        Ok(Self {
            handler,
            decoder,
            connection_id: connection_id.into(),
            stage: ConnectionStage::AwaitingHello,
            // 序号属于 transport connection，不跨断线延续。恢复依赖下一次 hello 的权威 snapshot。
            next_event_sequence: 1,
            max_frame_length,
        })
    }

    /// 接收任意边界的字节块，并返回所有应原样写回 transport 的完整帧。
    pub fn receive(&mut self, chunk: &[u8]) -> Result<Vec<Vec<u8>>, RpcError> {
        if self.stage == ConnectionStage::Closed {
            return Err(RpcError::Closed);
        }

        let payloads = self.decoder.push(chunk).map_err(frame_error)?;
        let mut outbound = Vec::new();
        for payload in payloads {
            let message = decode_cbor::<ClientMessage>(&payload).map_err(cbor_error)?;
            outbound.extend(self.handle_message(message)?);
            if self.stage == ConnectionStage::Closed {
                break;
            }
        }
        Ok(outbound)
    }

    /// 在 transport EOF 时验证不存在被截断的 length-prefixed frame。
    pub fn finish(&mut self) -> Result<(), RpcError> {
        self.decoder.end().map_err(frame_error)
    }

    /// 发送业务层异步产生的事件。只有 handshake 成功后事件才可见。
    pub fn flush_events(&mut self) -> Result<Vec<Vec<u8>>, RpcError> {
        if self.stage != ConnectionStage::Ready {
            return Ok(Vec::new());
        }
        self.handler
            .drain_events()
            .into_iter()
            .map(|event| {
                let sequence = self.next_event_sequence;
                self.next_event_sequence = self
                    .next_event_sequence
                    .checked_add(1)
                    .expect("event sequence must not overflow a u64 connection lifetime");
                self.encode(ServerMessage::Event(EventEnvelope {
                    message_type: EventMessageType::Event,
                    sequence,
                    event,
                }))
            })
            .collect()
    }

    /// 取得 handler 所有权，适合连接退出后由外层管理器回收会话资源。
    pub fn into_handler(self) -> H {
        self.handler
    }

    fn handle_message(&mut self, message: ClientMessage) -> Result<Vec<Vec<u8>>, RpcError> {
        match (self.stage, message) {
            (
                ConnectionStage::AwaitingHello,
                ClientMessage::Hello {
                    version,
                    last_seen_sequence: _,
                },
            ) => self.hello(version),
            (ConnectionStage::AwaitingHello, ClientMessage::Request { .. }) => {
                self.stage = ConnectionStage::Closed;
                Err(RpcError::InvalidMessage(
                    "The first client message must be hello".to_owned(),
                ))
            }
            (ConnectionStage::Ready, ClientMessage::Hello { .. }) => {
                self.stage = ConnectionStage::Closed;
                Err(RpcError::InvalidMessage(
                    "hello may only be sent as the first message".to_owned(),
                ))
            }
            (ConnectionStage::Ready, ClientMessage::Request { id, request }) => {
                self.request(id, request)
            }
            (ConnectionStage::Closed, _) => Err(RpcError::Closed),
        }
    }

    fn hello(&mut self, version: u32) -> Result<Vec<Vec<u8>>, RpcError> {
        if version != PROTOCOL_VERSION {
            self.stage = ConnectionStage::Closed;
            return Ok(vec![self.encode(ServerMessage::HelloError(
                ServerHelloError {
                    message_type: HelloErrorMessageType::HelloError,
                    error: ProtocolError {
                        code: ProtocolErrorCode::Version,
                        message: format!(
                            "Unsupported protocol version {version}; expected {PROTOCOL_VERSION}"
                        ),
                        details: None,
                    },
                },
            ))?]);
        }

        let hello = ServerMessage::Hello(ServerHello {
            message_type: HelloMessageType::Hello,
            version: PROTOCOL_VERSION,
            connection_id: self.connection_id.clone(),
            snapshot: self.handler.snapshot(),
        });
        self.stage = ConnectionStage::Ready;
        let mut outbound = vec![self.encode(hello)?];
        outbound.extend(self.flush_events()?);
        Ok(outbound)
    }

    fn request(&mut self, id: String, request: Command) -> Result<Vec<Vec<u8>>, RpcError> {
        let response = match self.handler.execute(request) {
            Ok(result) => ServerMessage::SuccessResponse(SuccessResponse::new(id, result)),
            Err(error) => ServerMessage::ErrorResponse(ErrorResponse::new(id, error)),
        };
        let mut outbound = vec![self.encode(response)?];
        outbound.extend(self.flush_events()?);
        Ok(outbound)
    }

    fn encode(&self, message: ServerMessage) -> Result<Vec<u8>, RpcError> {
        let payload = encode_cbor(&message).map_err(cbor_error)?;
        encode_frame(&payload, self.max_frame_length).map_err(frame_error)
    }
}

fn cbor_error(error: CborError) -> RpcError {
    RpcError::InvalidMessage(error.to_string())
}

fn frame_error(error: FrameError) -> RpcError {
    RpcError::Transport(error.to_string())
}

/// 在一个阻塞字节流上运行完整的 RPC 连接生命周期。
///
/// listener 所属 crate 负责创建 handler；本函数只处理分帧、写回、EOF 完整性校验和 handler
/// 回收。无论正常 EOF、协议错误还是底层 I/O 错误，都会将 handler 交给 `on_close`，从而让
/// Runtime 解除该连接的 Session 订阅。
pub fn serve_connection<H, R, W>(
    mut connection: RpcConnection<H>,
    mut reader: R,
    mut writer: W,
    mut on_close: impl FnMut(H),
) -> Result<(), RpcError>
where
    H: RpcCommandHandler,
    R: Read,
    W: Write,
{
    let mut buffer = [0_u8; 16 * 1024];
    let result = loop {
        let length = match reader.read(&mut buffer) {
            Ok(0) => match connection.finish() {
                Ok(()) => break Ok(()),
                Err(error) => break Err(error),
            },
            Ok(length) => length,
            Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
            Err(error) => break Err(RpcError::Transport(error.to_string())),
        };
        let frames = match connection.receive(&buffer[..length]) {
            Ok(frames) => frames,
            Err(error) => break Err(error),
        };
        let mut write_error = None;
        for frame in frames {
            if let Err(error) = writer.write_all(&frame) {
                write_error = Some(RpcError::Transport(error.to_string()));
                break;
            }
        }
        if let Some(error) = write_error {
            break Err(error);
        }
        if let Err(error) = writer.flush() {
            break Err(RpcError::Transport(error.to_string()));
        }
    };
    on_close(connection.into_handler());
    result
}

#[cfg(test)]
mod tests {
    use std::{
        io::Cursor,
        sync::{
            Arc,
            atomic::{AtomicBool, Ordering},
        },
    };

    use protocol::{
        ClientMessage, CommandResult, EventEnvelope, PROTOCOL_VERSION, ProtocolError,
        ProtocolErrorCode, ServerEvent, ServerMessage, ServerSnapshot,
        cbor::{decode_cbor, encode_cbor},
        framing::{decode_complete_frame, encode_frame},
    };

    use super::{RpcCommandHandler, RpcConnection, RpcError, serve_connection};

    struct FakeHandler {
        events: Vec<ServerEvent>,
        executed: Vec<String>,
    }

    impl FakeHandler {
        fn new(events: Vec<ServerEvent>) -> Self {
            Self {
                events,
                executed: Vec::new(),
            }
        }
    }

    impl RpcCommandHandler for FakeHandler {
        fn execute(&mut self, command: protocol::Command) -> Result<CommandResult, ProtocolError> {
            match command {
                protocol::Command::List => {
                    self.executed.push("list".to_owned());
                    Ok(CommandResult::List {
                        sessions: Vec::new(),
                    })
                }
                _ => Err(ProtocolError {
                    code: ProtocolErrorCode::NotImplemented,
                    message: "Fake handler only supports list".to_owned(),
                    details: None,
                }),
            }
        }

        fn snapshot(&self) -> ServerSnapshot {
            ServerSnapshot {
                server_id: "server-1".to_owned(),
                protocol_version: PROTOCOL_VERSION,
                revision: 3,
                sessions: Vec::new(),
                models: Vec::new(),
            }
        }

        fn drain_events(&mut self) -> Vec<ServerEvent> {
            std::mem::take(&mut self.events)
        }
    }

    struct CloseTrackingHandler {
        closed: Arc<AtomicBool>,
    }

    impl RpcCommandHandler for CloseTrackingHandler {
        fn execute(&mut self, _command: protocol::Command) -> Result<CommandResult, ProtocolError> {
            Err(ProtocolError {
                code: ProtocolErrorCode::NotImplemented,
                message: "test handler only supports hello".to_owned(),
                details: None,
            })
        }

        fn snapshot(&self) -> ServerSnapshot {
            FakeHandler::new(Vec::new()).snapshot()
        }

        fn drain_events(&mut self) -> Vec<ServerEvent> {
            Vec::new()
        }
    }

    fn client_frame(message: ClientMessage) -> Vec<u8> {
        let payload = encode_cbor(&message).expect("client message should encode");
        encode_frame(&payload, 1024).expect("client frame should encode")
    }

    fn server_messages(frames: Vec<Vec<u8>>) -> Vec<ServerMessage> {
        frames
            .iter()
            .map(|frame| {
                let payload = decode_complete_frame(frame, 1024).expect("complete server frame");
                decode_cbor(payload).expect("server message should decode")
            })
            .collect()
    }

    #[test]
    fn fragmented_hello_returns_snapshot_then_pending_events() {
        let event = ServerEvent::SessionRemoved {
            session_id: "session-1".to_owned(),
        };
        let mut connection = RpcConnection::with_max_frame_length(
            "connection-1",
            FakeHandler::new(vec![event]),
            1024,
        )
        .expect("connection should initialize");
        // 测试必须使用当前协议常量，避免协议版本升级后把成功握手误判为失败。
        let frame = client_frame(ClientMessage::Hello {
            version: PROTOCOL_VERSION,
            last_seen_sequence: None,
        });

        assert!(
            connection
                .receive(&frame[..3])
                .expect("fragment is valid")
                .is_empty()
        );
        let messages = server_messages(
            connection
                .receive(&frame[3..])
                .expect("hello should succeed"),
        );

        assert!(matches!(messages[0], ServerMessage::Hello(_)));
        assert!(matches!(
            messages[1],
            ServerMessage::Event(EventEnvelope { sequence: 1, .. })
        ));
    }

    #[test]
    fn request_returns_response_before_events_in_same_connection_order() {
        let mut connection = RpcConnection::with_max_frame_length(
            "connection-1",
            FakeHandler::new(vec![ServerEvent::SessionRemoved {
                session_id: "session-1".to_owned(),
            }]),
            1024,
        )
        .expect("connection should initialize");
        connection
            .receive(&client_frame(ClientMessage::Hello {
                version: PROTOCOL_VERSION,
                last_seen_sequence: None,
            }))
            .expect("hello should succeed");
        let messages = server_messages(
            connection
                .receive(&client_frame(ClientMessage::Request {
                    id: "req-1".to_owned(),
                    request: protocol::Command::List,
                }))
                .expect("request should succeed"),
        );

        assert!(matches!(
            messages.as_slice(),
            [ServerMessage::SuccessResponse(_)]
        ));
        assert_eq!(connection.into_handler().executed, vec!["list"]);
    }

    #[test]
    fn fresh_connection_restarts_event_sequence_after_reconnect_snapshot() {
        let event = ServerEvent::SessionRemoved {
            session_id: "session-1".to_owned(),
        };
        let hello = ClientMessage::Hello {
            version: PROTOCOL_VERSION,
            // 旧连接的已应用边界只用于声明客户端状态；新的权威 hello snapshot 后，
            // 新 transport 的 event sequence 必须从 1 开始。
            last_seen_sequence: Some(7),
        };
        let mut reconnected = RpcConnection::with_max_frame_length(
            "connection-2",
            FakeHandler::new(vec![event]),
            1024,
        )
        .expect("connection should initialize");

        let messages = server_messages(
            reconnected
                .receive(&client_frame(hello))
                .expect("reconnect hello should succeed"),
        );

        assert!(matches!(messages[0], ServerMessage::Hello(_)));
        assert!(matches!(
            messages[1],
            ServerMessage::Event(EventEnvelope { sequence: 1, .. })
        ));
    }

    #[test]
    fn incompatible_version_returns_hello_error_and_closes_connection() {
        let mut connection = RpcConnection::with_max_frame_length(
            "connection-1",
            FakeHandler::new(Vec::new()),
            1024,
        )
        .expect("connection should initialize");
        let messages = server_messages(
            connection
                // 选择一个与当前版本不同的值，保证版本协商拒绝逻辑在版本演进后仍被覆盖。
                .receive(&client_frame(ClientMessage::Hello {
                    version: PROTOCOL_VERSION.saturating_add(1),
                    last_seen_sequence: None,
                }))
                .expect("version response should encode"),
        );

        assert!(matches!(
            messages.as_slice(),
            [ServerMessage::HelloError(_)]
        ));
        assert_eq!(connection.receive(&[]), Err(RpcError::Closed));
    }

    #[test]
    fn truncated_frame_returns_handler_to_close_callback() {
        let closed = Arc::new(AtomicBool::new(false));
        let connection = RpcConnection::new(
            "connection-1",
            CloseTrackingHandler {
                closed: Arc::clone(&closed),
            },
        );

        let error = serve_connection(
            connection,
            Cursor::new(vec![0, 0, 0, 1]),
            Vec::new(),
            |handler| handler.closed.store(true, Ordering::SeqCst),
        )
        .expect_err("truncated frame must fail");

        assert!(matches!(error, RpcError::Transport(_)));
        assert!(closed.load(Ordering::SeqCst));
    }

    #[test]
    fn rejects_request_before_hello_without_executing_handler() {
        let mut connection = RpcConnection::with_max_frame_length(
            "connection-1",
            FakeHandler::new(Vec::new()),
            1024,
        )
        .expect("connection should initialize");
        let error = connection
            .receive(&client_frame(ClientMessage::Request {
                id: "req-1".to_owned(),
                request: protocol::Command::List,
            }))
            .expect_err("request before hello must fail");

        assert!(matches!(error, RpcError::InvalidMessage(_)));
        assert!(connection.into_handler().executed.is_empty());
    }
}

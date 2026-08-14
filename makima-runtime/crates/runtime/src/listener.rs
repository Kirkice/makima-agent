//! Runtime 层的真实 RPC listener。
//!
//! RPC crate 仅拥有单连接 framing 状态机；本模块负责为 stdio 或 Unix socket 创建连接级
//! handler，并在 transport 终止后调用其 `disconnect`，从共享 SessionManager 解除订阅。

use std::io::{self, Read, Write};

#[cfg(unix)]
use std::{sync::Arc, thread};

use rpc::{RpcCommandHandler, RpcConnection, RpcError, serve_connection};

use crate::session_manager::{ConnectionSessionHandler, SessionFactory};

/// 为一个新 transport 连接创建业务 handler 的工厂。
pub trait RpcHandlerFactory<H>: Send + Sync + 'static {
    /// 使用 listener 分配的连接标识创建 handler。
    fn create(&self, connection_id: String) -> H;
}

impl<H, F> RpcHandlerFactory<H> for F
where
    F: Fn(String) -> H + Send + Sync + 'static,
{
    fn create(&self, connection_id: String) -> H {
        self(connection_id)
    }
}

/// 所有由 Runtime listener 管理的 handler 都必须在断线时释放连接状态。
pub trait DisconnectableRpcHandler: RpcCommandHandler {
    /// 释放此 transport 对应的运行时订阅；必须允许重复调用。
    fn disconnect(&mut self);
}

impl<F> DisconnectableRpcHandler for ConnectionSessionHandler<F>
where
    F: SessionFactory,
{
    fn disconnect(&mut self) {
        ConnectionSessionHandler::disconnect(self);
    }
}

/// 在已建立的阻塞流上提供一条 RPC 连接。
pub fn serve_rpc_stream<H, R, W>(
    connection_id: impl Into<String>,
    handler: H,
    reader: R,
    writer: W,
) -> Result<(), RpcError>
where
    H: DisconnectableRpcHandler,
    R: Read,
    W: Write,
{
    let connection = RpcConnection::new(connection_id, handler);
    serve_connection(connection, reader, writer, |mut handler| {
        handler.disconnect()
    })
}

/// 在 stdio 上提供单一 RPC 连接。
///
/// 调用方应只在独占 stdin/stdout 的子进程模式调用本函数；所有协议输出写入 stdout，诊断输出
/// 必须由调用方写入 stderr，避免污染长度前缀字节流。
pub fn serve_stdio<H>(connection_id: impl Into<String>, handler: H) -> Result<(), RpcError>
where
    H: DisconnectableRpcHandler,
{
    serve_rpc_stream(connection_id, handler, io::stdin(), io::stdout())
}

/// Unix socket listener 的运行配置。
#[cfg(unix)]
#[derive(Debug, Clone)]
pub struct UnixSocketListenerOptions {
    /// bind 的 socket 文件路径。
    pub path: std::path::PathBuf,
}

/// 在 Unix socket 上持续接收连接。
///
/// 每个 socket 使用独立 OS 线程；共享 manager 的同步由 `ConnectionSessionHandler` 的
/// `Arc<Mutex<_>>` 完成。accept 循环仅在 listener 被外部关闭时退出。
#[cfg(unix)]
pub fn serve_unix_socket<H>(
    options: UnixSocketListenerOptions,
    factory: Arc<impl RpcHandlerFactory<H>>,
) -> io::Result<()>
where
    H: DisconnectableRpcHandler + Send + 'static,
{
    use std::os::unix::net::UnixListener;

    if options.path.exists() {
        std::fs::remove_file(&options.path)?;
    }
    let listener = UnixListener::bind(&options.path)?;
    for accepted in listener.incoming() {
        let stream = accepted?;
        let factory = Arc::clone(&factory);
        let connection_id = format!("unix-{:?}", stream.peer_addr().ok());
        thread::spawn(move || {
            let reader = match stream.try_clone() {
                Ok(reader) => reader,
                Err(_) => return,
            };
            let handler = factory.create(connection_id.clone());
            let _ = serve_rpc_stream(connection_id, handler, reader, stream);
        });
    }
    Ok(())
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
        ClientMessage, CommandResult, ProtocolError, ProtocolErrorCode, ServerEvent,
        ServerSnapshot, cbor::encode_cbor, framing::encode_frame,
    };
    use rpc::RpcCommandHandler;

    use super::{DisconnectableRpcHandler, serve_rpc_stream};

    struct Handler {
        disconnected: Arc<AtomicBool>,
    }

    impl RpcCommandHandler for Handler {
        fn execute(&mut self, _command: protocol::Command) -> Result<CommandResult, ProtocolError> {
            Err(ProtocolError {
                code: ProtocolErrorCode::NotImplemented,
                message: "test handler only supports hello".to_owned(),
                details: None,
            })
        }

        fn snapshot(&self) -> ServerSnapshot {
            ServerSnapshot {
                server_id: "test".to_owned(),
                protocol_version: protocol::PROTOCOL_VERSION,
                revision: 0,
                sessions: Vec::new(),
                models: Vec::new(),
            }
        }

        fn drain_events(&mut self) -> Vec<ServerEvent> {
            Vec::new()
        }
    }

    impl DisconnectableRpcHandler for Handler {
        fn disconnect(&mut self) {
            self.disconnected.store(true, Ordering::SeqCst);
        }
    }

    #[test]
    fn stdio_style_eof_returns_handler_to_disconnect_lifecycle() {
        let hello = encode_frame(
            &encode_cbor(&ClientMessage::Hello {
                version: protocol::PROTOCOL_VERSION,
            })
            .expect("encode hello"),
            protocol::framing::DEFAULT_MAX_FRAME_LENGTH,
        )
        .expect("frame hello");
        let disconnected = Arc::new(AtomicBool::new(false));
        let result = serve_rpc_stream(
            "test-connection",
            Handler {
                disconnected: Arc::clone(&disconnected),
            },
            Cursor::new(hello),
            Vec::new(),
        );
        assert!(result.is_ok());
        assert!(disconnected.load(Ordering::SeqCst));
    }
}

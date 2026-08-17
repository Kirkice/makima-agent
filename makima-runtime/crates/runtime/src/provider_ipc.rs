//! Rust Core 到 TypeScript Provider Host 的 framed CBOR 传输。
//!
//! 此模块只实现进程边界的请求、响应和取消关联，不构造 Provider 请求，也不驱动
//! `AgentLoopEngine`。上层运行时在开始一轮模型调用时发送 request，并把读到的 `event`
//! 投递给 Agent Loop；因此 Agent Loop 保持没有网络或进程 I/O 的纯状态机。

use std::{
    collections::BTreeSet,
    ffi::OsStr,
    fmt,
    io::{self, Read, Write},
    process::{Child, ChildStdin, ChildStdout, Command, Stdio},
    sync::mpsc::{self, Receiver, TryRecvError},
    thread::{self, JoinHandle},
};

use protocol::{
    ProviderHostRequest, ProviderHostResponse, ProviderRequest,
    cbor::{decode_cbor, encode_cbor},
    framing::{DEFAULT_MAX_FRAME_LENGTH, FrameDecoder, encode_frame},
};

use crate::provider_runtime::ProviderStreamPort;

/// Provider Host 进程边界无法继续时的错误。
#[derive(Debug)]
pub enum ProviderIpcError {
    /// 底层管道发生读写错误，或 EOF 中含有截断帧。
    Transport(String),
    /// CBOR 负载无法解码为严格的共享 DTO。
    InvalidMessage(String),
    /// Host 为没有活动 request 的 ID 发送了响应。
    UnknownRequestId(String),
    /// request ID 已处于活动状态，不能并发复用。
    DuplicateRequestId(String),
}

impl fmt::Display for ProviderIpcError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Transport(message) => {
                write!(formatter, "Provider Host transport error: {message}")
            }
            Self::InvalidMessage(message) => {
                write!(formatter, "Invalid Provider Host message: {message}")
            }
            Self::UnknownRequestId(request_id) => {
                write!(
                    formatter,
                    "Provider Host responded for inactive request: {request_id}"
                )
            }
            Self::DuplicateRequestId(request_id) => {
                write!(
                    formatter,
                    "Provider request ID is already active: {request_id}"
                )
            }
        }
    }
}

impl std::error::Error for ProviderIpcError {}

/// 已启动的 TypeScript Provider Host 子进程及其 IPC 管道。
///
/// 子进程的 stdout 被保留给 framed CBOR 响应；Provider Host 的诊断必须写到 stderr，
/// 从而不会损坏协议流。调用 [`ProviderHostProcess::shutdown`] 会关闭 stdin；若 Host
/// 尚未退出，则强制终止并等待它。调用方必须在丢弃该值前显式调用该方法，以回收子进程。
pub struct ProviderHostProcess {
    child: Child,
    connection: ProviderHostConnection<ChildStdout, ChildStdin>,
}

impl ProviderHostProcess {
    /// 启动一个已经构建的 Provider Host 可执行入口。
    ///
    /// `program` 通常是 Node 可执行文件，`args` 应包含 Provider Host 的构建后入口，例如
    /// `packages/provider-host/dist/main.js`。当前环境、工作目录和 stderr 会从 Core 继承，
    /// 以便凭证和诊断遵循调用 Core 的进程上下文。
    pub fn spawn<I, S>(program: impl AsRef<OsStr>, args: I) -> Result<Self, ProviderIpcError>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<OsStr>,
    {
        let mut command = Command::new(program);
        command
            .args(args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit());
        let mut child = command
            .spawn()
            .map_err(|error| ProviderIpcError::Transport(error.to_string()))?;
        let reader = child.stdout.take().ok_or_else(|| {
            ProviderIpcError::Transport("Provider Host stdout pipe is unavailable".to_owned())
        })?;
        let writer = child.stdin.take().ok_or_else(|| {
            ProviderIpcError::Transport("Provider Host stdin pipe is unavailable".to_owned())
        })?;
        Ok(Self {
            child,
            connection: ProviderHostConnection::new(reader, writer),
        })
    }

    /// 返回 Provider IPC 客户端。
    pub fn connection_mut(&mut self) -> &mut ProviderHostConnection<ChildStdout, ChildStdin> {
        &mut self.connection
    }

    /// 返回 Host 是否已经退出；仍在运行时返回 `None`。
    pub fn try_wait(&mut self) -> Result<Option<std::process::ExitStatus>, ProviderIpcError> {
        self.child
            .try_wait()
            .map_err(|error| ProviderIpcError::Transport(error.to_string()))
    }

    /// 关闭输入；若 Host 尚未退出，则终止该进程并等待它退出。
    pub fn shutdown(mut self) -> Result<std::process::ExitStatus, ProviderIpcError> {
        drop(self.connection);
        match self
            .child
            .try_wait()
            .map_err(|error| ProviderIpcError::Transport(error.to_string()))?
        {
            Some(status) => Ok(status),
            None => {
                self.child
                    .kill()
                    .map_err(|error| ProviderIpcError::Transport(error.to_string()))?;
                self.child
                    .wait()
                    .map_err(|error| ProviderIpcError::Transport(error.to_string()))
            }
        }
    }
}

/// 从 Provider Host stdout 解码响应的只读端。
///
/// 它不维护 request 活动集合：该状态在写入端维护，而运行时 driver 会再次验证所有收到的
/// request ID。将读取端独立出来后，阻塞的 stdout 读取可以安全地放在专用线程，不会阻塞
/// request 或 abort 写入。
pub struct ProviderHostResponseReader<R> {
    reader: R,
    decoder: FrameDecoder,
}

impl<R> ProviderHostResponseReader<R>
where
    R: Read,
{
    /// 从 Provider Host stdout 创建 framed-CBOR 响应解码器。
    pub fn new(reader: R) -> Self {
        Self {
            reader,
            decoder: FrameDecoder::new(),
        }
    }

    /// 阻塞至收到至少一个完整响应或 stdout EOF。
    pub fn receive(&mut self) -> Result<Vec<ProviderHostResponse>, ProviderIpcError> {
        let mut buffer = [0_u8; 16 * 1024];
        loop {
            let length = match self.reader.read(&mut buffer) {
                Ok(0) => {
                    self.decoder
                        .end()
                        .map_err(|error| ProviderIpcError::Transport(error.to_string()))?;
                    return Ok(Vec::new());
                }
                Ok(length) => length,
                Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
                Err(error) => return Err(ProviderIpcError::Transport(error.to_string())),
            };
            let payloads = self
                .decoder
                .push(&buffer[..length])
                .map_err(|error| ProviderIpcError::Transport(error.to_string()))?;
            if payloads.is_empty() {
                continue;
            }
            return payloads
                .into_iter()
                .map(|payload| {
                    decode_cbor::<ProviderHostResponse>(&payload)
                        .map_err(|error| ProviderIpcError::InvalidMessage(error.to_string()))
                })
                .collect();
        }
    }
}

/// 非阻塞的 Provider stream port。
///
/// stdin 写入保留在调用 SessionManager 的线程；stdout 则由专用 reader 线程阻塞读取，并把
/// 已解码响应批次投递到 channel。因此 [`ProviderStreamPort::try_receive`] 从不等待模型输出。
pub struct ProviderHostStreamPort {
    writer: Option<ProviderHostConnection<io::Empty, ChildStdin>>,
    responses: Receiver<Result<Vec<ProviderHostResponse>, ProviderIpcError>>,
    child: Option<Child>,
    reader_task: Option<JoinHandle<()>>,
}

impl ProviderHostStreamPort {
    /// 启动 Provider Host，并将其 stdout 交给后台 framed-CBOR reader。
    pub fn spawn<I, S>(program: impl AsRef<OsStr>, args: I) -> Result<Self, ProviderIpcError>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<OsStr>,
    {
        let mut command = Command::new(program);
        command
            .args(args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit());
        let mut child = command
            .spawn()
            .map_err(|error| ProviderIpcError::Transport(error.to_string()))?;
        let stdout = child.stdout.take().ok_or_else(|| {
            ProviderIpcError::Transport("Provider Host stdout pipe is unavailable".to_owned())
        })?;
        let stdin = child.stdin.take().ok_or_else(|| {
            ProviderIpcError::Transport("Provider Host stdin pipe is unavailable".to_owned())
        })?;
        let (sender, responses) = mpsc::channel();
        let reader_task = thread::spawn(move || {
            let mut reader = ProviderHostResponseReader::new(stdout);
            loop {
                let result = reader.receive();
                let should_stop =
                    matches!(result, Ok(ref batch) if batch.is_empty()) || result.is_err();
                if sender.send(result).is_err() || should_stop {
                    break;
                }
            }
        });
        Ok(Self {
            writer: Some(ProviderHostConnection::new(io::empty(), stdin)),
            responses,
            child: Some(child),
            reader_task: Some(reader_task),
        })
    }

    fn shutdown(&mut self) {
        drop(self.writer.take());
        if let Some(mut child) = self.child.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
        if let Some(task) = self.reader_task.take() {
            let _ = task.join();
        }
    }
}

impl ProviderStreamPort for ProviderHostStreamPort {
    fn request(&mut self, request: ProviderRequest) -> Result<(), String> {
        self.writer
            .as_mut()
            .ok_or_else(|| "Provider Host transport is shut down".to_owned())?
            .request(request)
            .map_err(|error| error.to_string())
    }

    fn abort(&mut self, request_id: &str) -> Result<(), String> {
        self.writer
            .as_mut()
            .ok_or_else(|| "Provider Host transport is shut down".to_owned())?
            .abort(request_id)
            .map(|_| ())
            .map_err(|error| error.to_string())
    }

    fn try_receive(&mut self) -> Result<Option<Vec<ProviderHostResponse>>, String> {
        match self.responses.try_recv() {
            Ok(Ok(batch)) if batch.is_empty() => {
                Err("Provider Host closed stdout before the active request completed".to_owned())
            }
            Ok(Ok(batch)) => Ok(Some(batch)),
            Ok(Err(error)) => Err(error.to_string()),
            Err(TryRecvError::Empty) => Ok(None),
            Err(TryRecvError::Disconnected) => {
                Err("Provider Host reader stopped unexpectedly".to_owned())
            }
        }
    }
}

impl Drop for ProviderHostStreamPort {
    fn drop(&mut self) {
        self.shutdown();
    }
}

/// 面向一条 Provider Host stdin/stdout 管道的同步 IPC 客户端。
///
/// 一条连接可以同时存在多个活动请求。调用 [`ProviderHostConnection::receive`] 时会读取到至少
/// 一帧响应或 EOF；`complete` 是唯一把 request ID 移出活动集合的消息。调用
/// [`ProviderHostConnection::abort`] 多次只会重复发送幂等的 abort 请求，不会改变本地活动状态。
pub struct ProviderHostConnection<R, W> {
    reader: R,
    writer: W,
    decoder: FrameDecoder,
    active_request_ids: BTreeSet<String>,
}

impl<R, W> ProviderHostConnection<R, W>
where
    R: Read,
    W: Write,
{
    /// 使用共享 16 MiB framing 上限创建连接。
    pub fn new(reader: R, writer: W) -> Self {
        Self {
            reader,
            writer,
            decoder: FrameDecoder::new(),
            active_request_ids: BTreeSet::new(),
        }
    }

    /// 发送一条新的 Provider 请求。
    ///
    /// request ID 在对应 `complete` 到达前不能重复使用，防止 Host 输出被路由到错误的 Agent
    /// 回合。写入成功后才将 ID 标记为 active。
    pub fn request(&mut self, request: ProviderRequest) -> Result<(), ProviderIpcError> {
        if self.active_request_ids.contains(&request.request_id) {
            return Err(ProviderIpcError::DuplicateRequestId(request.request_id));
        }
        let request_id = request.request_id.clone();
        self.send(ProviderHostRequest::Request { request })?;
        self.active_request_ids.insert(request_id);
        Ok(())
    }

    /// 为仍在运行的请求发送取消信号。
    ///
    /// 未知或已完成 ID 返回 `false` 且不会写入管道；活动 ID 返回 `true`。Host 对重复 abort
    /// 需保持幂等，因此调用方可在本地取消重试时安全再次调用本方法。
    pub fn abort(&mut self, request_id: &str) -> Result<bool, ProviderIpcError> {
        if !self.active_request_ids.contains(request_id) {
            return Ok(false);
        }
        self.send(ProviderHostRequest::Abort {
            request_id: request_id.to_owned(),
        })?;
        Ok(true)
    }

    /// 返回一个 request ID 是否正在等待 Host 的 `complete`。
    pub fn is_active(&self, request_id: &str) -> bool {
        self.active_request_ids.contains(request_id)
    }

    /// 从 Host 读取并验证下一批完整响应。
    ///
    /// 正常 EOF 仅在 frame 边界上允许，并返回空列表；若 EOF 截断了 frame，则返回 transport
    /// 错误。上层收到空列表后应按自身进程生命周期将所有未完成请求结算为失败或取消。
    pub fn receive(&mut self) -> Result<Vec<ProviderHostResponse>, ProviderIpcError> {
        let mut buffer = [0_u8; 16 * 1024];
        loop {
            let length = match self.reader.read(&mut buffer) {
                Ok(0) => {
                    self.decoder
                        .end()
                        .map_err(|error| ProviderIpcError::Transport(error.to_string()))?;
                    return Ok(Vec::new());
                }
                Ok(length) => length,
                Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
                Err(error) => return Err(ProviderIpcError::Transport(error.to_string())),
            };
            let payloads = self
                .decoder
                .push(&buffer[..length])
                .map_err(|error| ProviderIpcError::Transport(error.to_string()))?;
            if payloads.is_empty() {
                continue;
            }

            let mut responses = Vec::with_capacity(payloads.len());
            for payload in payloads {
                let response = decode_cbor::<ProviderHostResponse>(&payload)
                    .map_err(|error| ProviderIpcError::InvalidMessage(error.to_string()))?;
                self.accept_response(&response)?;
                responses.push(response);
            }
            return Ok(responses);
        }
    }

    /// 取回底层管道，供进程关闭或测试检查写出的帧。
    pub fn into_inner(self) -> (R, W) {
        (self.reader, self.writer)
    }

    fn send(&mut self, message: ProviderHostRequest) -> Result<(), ProviderIpcError> {
        let payload = encode_cbor(&message)
            .map_err(|error| ProviderIpcError::InvalidMessage(error.to_string()))?;
        let frame = encode_frame(&payload, DEFAULT_MAX_FRAME_LENGTH)
            .map_err(|error| ProviderIpcError::Transport(error.to_string()))?;
        self.writer
            .write_all(&frame)
            .map_err(|error| ProviderIpcError::Transport(error.to_string()))?;
        self.writer
            .flush()
            .map_err(|error| ProviderIpcError::Transport(error.to_string()))
    }

    fn accept_response(&mut self, response: &ProviderHostResponse) -> Result<(), ProviderIpcError> {
        let request_id = match response {
            ProviderHostResponse::Event { request_id, .. }
            | ProviderHostResponse::Complete { request_id } => request_id,
        };
        if !self.active_request_ids.contains(request_id) {
            return Err(ProviderIpcError::UnknownRequestId(request_id.clone()));
        }
        if let ProviderHostResponse::Complete { request_id } = response {
            self.active_request_ids.remove(request_id);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;

    use protocol::{
        AssistantStopReason, ModelRef, ProviderHostResponse, ProviderRequest, ProviderStreamEvent,
        cbor::{decode_cbor, encode_cbor},
        framing::{DEFAULT_MAX_FRAME_LENGTH, decode_complete_frame, encode_frame},
    };

    use super::{ProviderHostConnection, ProviderIpcError};

    fn request(request_id: &str) -> ProviderRequest {
        ProviderRequest {
            request_id: request_id.to_owned(),
            model: ModelRef {
                provider: "test".to_owned(),
                id: "model".to_owned(),
            },
            system_prompt: "system".to_owned(),
            messages: Vec::new(),
            tools: Vec::new(),
        }
    }

    fn response_frame(response: ProviderHostResponse) -> Vec<u8> {
        let payload = encode_cbor(&response).expect("response should encode");
        encode_frame(&payload, DEFAULT_MAX_FRAME_LENGTH).expect("response should frame")
    }

    #[test]
    fn routes_events_until_complete_and_writes_idempotent_abort_frames() {
        let mut responses = response_frame(ProviderHostResponse::Event {
            request_id: "request-1".to_owned(),
            event: ProviderStreamEvent::Start {
                message_id: "assistant-1".to_owned(),
                timestamp: 10,
            },
        });
        responses.extend(response_frame(ProviderHostResponse::Complete {
            request_id: "request-1".to_owned(),
        }));
        let mut connection = ProviderHostConnection::new(Cursor::new(responses), Vec::new());

        connection
            .request(request("request-1"))
            .expect("request writes");
        assert!(connection.abort("request-1").expect("abort writes"));
        assert!(
            connection
                .abort("request-1")
                .expect("repeated abort writes")
        );
        assert!(connection.is_active("request-1"));

        let received = connection.receive().expect("responses decode");
        assert_eq!(received.len(), 2);
        assert!(matches!(
            received.as_slice(),
            [
                ProviderHostResponse::Event {
                    event: ProviderStreamEvent::Start { .. },
                    ..
                },
                ProviderHostResponse::Complete { .. }
            ]
        ));
        assert!(!connection.is_active("request-1"));
        assert!(
            !connection
                .abort("request-1")
                .expect("completed abort is ignored")
        );

        let (_reader, writer) = connection.into_inner();
        let first_end =
            4 + u32::from_be_bytes([writer[0], writer[1], writer[2], writer[3]]) as usize;
        let first = decode_complete_frame(&writer[..first_end], DEFAULT_MAX_FRAME_LENGTH)
            .expect("first outbound frame");
        let first: protocol::ProviderHostRequest = decode_cbor(first).expect("request decodes");
        assert!(matches!(
            first,
            protocol::ProviderHostRequest::Request { .. }
        ));
        let second_end = first_end
            + 4
            + u32::from_be_bytes([
                writer[first_end],
                writer[first_end + 1],
                writer[first_end + 2],
                writer[first_end + 3],
            ]) as usize;
        let second =
            decode_complete_frame(&writer[first_end..second_end], DEFAULT_MAX_FRAME_LENGTH)
                .expect("second outbound frame");
        let second: protocol::ProviderHostRequest =
            decode_cbor(second).expect("first abort decodes");
        assert!(matches!(
            second,
            protocol::ProviderHostRequest::Abort { .. }
        ));
        let third = decode_complete_frame(&writer[second_end..], DEFAULT_MAX_FRAME_LENGTH)
            .expect("third outbound frame");
        let third: protocol::ProviderHostRequest =
            decode_cbor(third).expect("second abort decodes");
        assert!(matches!(third, protocol::ProviderHostRequest::Abort { .. }));
    }

    #[test]
    fn response_reader_decodes_complete_framed_batches() {
        let mut bytes = response_frame(ProviderHostResponse::Event {
            request_id: "request-1".to_owned(),
            event: ProviderStreamEvent::Start {
                message_id: "assistant-1".to_owned(),
                timestamp: 10,
            },
        });
        bytes.extend(response_frame(ProviderHostResponse::Complete {
            request_id: "request-1".to_owned(),
        }));
        let mut reader = super::ProviderHostResponseReader::new(Cursor::new(bytes));
        let responses = reader.receive().expect("responses decode");
        assert!(matches!(
            responses.as_slice(),
            [
                ProviderHostResponse::Event { .. },
                ProviderHostResponse::Complete { .. }
            ]
        ));
        assert!(reader.receive().expect("EOF is valid").is_empty());
    }

    #[test]
    fn rejects_unknown_responses_and_duplicate_active_request_ids() {
        let response = response_frame(ProviderHostResponse::Event {
            request_id: "unknown".to_owned(),
            event: ProviderStreamEvent::Done {
                message_id: "assistant-1".to_owned(),
                content: Vec::new(),
                response_model: None,
                usage: protocol::Usage {
                    input: 0,
                    output: 0,
                    cache_read: 0,
                    cache_write: 0,
                    reasoning: None,
                    total_tokens: 0,
                    cost: protocol::UsageCost {
                        input: 0.0,
                        output: 0.0,
                        cache_read: 0.0,
                        cache_write: 0.0,
                        total: 0.0,
                    },
                },
                timestamp: 20,
                stop_reason: AssistantStopReason::Stop,
            },
        });
        let mut connection = ProviderHostConnection::new(Cursor::new(response), Vec::new());
        connection
            .request(request("request-1"))
            .expect("request writes");
        assert!(matches!(
            connection.receive(),
            Err(ProviderIpcError::UnknownRequestId(request_id)) if request_id == "unknown"
        ));
        assert!(matches!(
            connection.request(request("request-1")),
            Err(ProviderIpcError::DuplicateRequestId(request_id)) if request_id == "request-1"
        ));
    }
}

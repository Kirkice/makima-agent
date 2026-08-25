//! Rust Core 的生产 Tool Runtime。
//!
//! Agent Loop 只决定何时调用工具；本 crate 负责工具目录、Provider 可见定义、参数校验、
//! 取消、超时以及稳定结果归一化。具体工具只依赖 [`ToolExecutionContext`]，不会接触
//! Provider、Session Store 或 RPC，从而可以独立测试和替换。

mod read;

use std::{
    collections::{BTreeSet, VecDeque},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc::{self, Receiver, Sender, TryRecvError},
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

use agent_loop::{ToolRuntimePort, ToolRuntimePortEvent};
use protocol::{TextOrImageContent, ToolCall, ToolDefinition, ToolResult};
use serde_json::Value;

pub use read::ReadTool;

/// 工具执行过程中可观察的稳定生命周期事件。
#[derive(Debug, Clone, PartialEq)]
pub enum ToolRuntimeEvent {
    Started {
        tool_call: ToolCall,
    },
    Updated {
        tool_call_id: String,
        content: Vec<TextOrImageContent>,
        details: Option<Value>,
    },
    Finished {
        result: ToolResult,
    },
}

/// 一次工具批次共享的取消令牌。
///
/// 使用独立类型而不是暴露 `AtomicBool`，避免工具实现依赖运行时内部同步细节。
#[derive(Debug, Clone, Default)]
pub struct CancellationToken {
    cancelled: Arc<AtomicBool>,
}

impl CancellationToken {
    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::Release);
    }

    pub fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::Acquire)
    }
}

/// 单次工具调用的只读执行上下文。
#[derive(Debug, Clone)]
pub struct ToolExecutionContext {
    cancellation: CancellationToken,
    deadline: Option<Instant>,
    updates: Sender<WorkerEvent>,
    tool_call_id: String,
}

impl ToolExecutionContext {
    fn new(
        cancellation: CancellationToken,
        deadline: Option<Instant>,
        updates: Sender<WorkerEvent>,
        tool_call_id: String,
    ) -> Self {
        Self {
            cancellation,
            deadline,
            updates,
            tool_call_id,
        }
    }

    /// 发布可丢弃的运行中完整快照。Runtime 在调用已结算后会隔离迟到更新。
    pub fn report_update(&self, output: ToolOutput) {
        let _ = self.updates.send(WorkerEvent::Updated {
            tool_call_id: self.tool_call_id.clone(),
            output,
        });
    }

    /// 在 I/O 边界调用，统一产生与工具实现无关的取消/超时错误。
    pub fn check_active(&self) -> Result<(), ToolExecutionError> {
        if self.cancellation.is_cancelled() {
            return Err(ToolExecutionError::new("Tool execution aborted"));
        }
        if self
            .deadline
            .is_some_and(|deadline| Instant::now() >= deadline)
        {
            return Err(ToolExecutionError::new("Tool execution timed out"));
        }
        Ok(())
    }
}

/// 单个工具的生产边界。工具必须可跨 Session worker 安全持有。
pub trait Tool: Send + Sync {
    /// 返回完整 Provider 声明；名称同时是 Runtime 的唯一注册键。
    fn definition(&self) -> ToolDefinition;

    /// 执行一次已通过通用 JSON Schema 校验的调用。
    fn execute(
        &self,
        call: &ToolCall,
        context: &ToolExecutionContext,
    ) -> Result<ToolOutput, ToolExecutionError>;
}

/// 成功工具调用产生的内容与可选 JSON 详情。
#[derive(Debug, Clone, PartialEq)]
pub struct ToolOutput {
    pub content: Vec<TextOrImageContent>,
    pub details: Option<Value>,
}

impl ToolOutput {
    pub fn text(text: impl Into<String>) -> Self {
        Self {
            content: vec![TextOrImageContent::Text { text: text.into() }],
            details: None,
        }
    }
}

/// 可安全写入 transcript 的工具错误。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolExecutionError {
    message: String,
}

impl ToolExecutionError {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }

    pub fn message(&self) -> &str {
        &self.message
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ToolRuntimeError {
    DuplicateToolName(String),
    EmptyToolName,
    InvalidToolSchema(String),
    BatchAlreadyActive,
}

#[derive(Debug)]
enum WorkerEvent {
    Updated {
        tool_call_id: String,
        output: ToolOutput,
    },
    Finished(Result<ToolOutput, ToolExecutionError>),
}

struct ActiveExecution {
    call: ToolCall,
    timestamp: u64,
    deadline: Option<Instant>,
    receiver: Receiver<WorkerEvent>,
    worker: JoinHandle<()>,
}

/// Session-owned Tool Runtime。
///
/// 每个调用在独立 worker 中执行，Session 线程只做非阻塞轮询。并行批次会同时启动所有
/// 调用；任何 `sequential` 工具都会让整批保守降级为串行。Rust 无法安全强杀任意线程，
/// 因此 timeout/abort 会立即产生唯一逻辑终态并隔离迟到输出；worker 自然返回后由
/// `reap_workers` 回收。所有 finished 事件按 Provider source order 输出，即使 worker 的
/// 实际完成顺序相反。
pub struct ToolRuntime {
    tools: Vec<Arc<dyn Tool>>,
    cancellation: CancellationToken,
    timeout: Option<Duration>,
    pending: VecDeque<ToolCall>,
    active: Vec<ActiveExecution>,
    completed: std::collections::BTreeMap<String, ToolResult>,
    result_order: VecDeque<String>,
    serial_batch: bool,
    retired_workers: Vec<JoinHandle<()>>,
    batch_timestamp: u64,
}

impl ToolRuntime {
    pub fn new() -> Self {
        Self {
            tools: Vec::new(),
            cancellation: CancellationToken::default(),
            timeout: Some(Duration::from_secs(120)),
            pending: VecDeque::new(),
            active: Vec::new(),
            completed: std::collections::BTreeMap::new(),
            result_order: VecDeque::new(),
            serial_batch: false,
            retired_workers: Vec::new(),
            batch_timestamp: 0,
        }
    }

    pub fn with_timeout(mut self, timeout: Option<Duration>) -> Self {
        self.timeout = timeout;
        self
    }

    pub fn register(&mut self, tool: impl Tool + 'static) -> Result<(), ToolRuntimeError> {
        let definition = tool.definition();
        if definition.name.is_empty() {
            return Err(ToolRuntimeError::EmptyToolName);
        }
        validate_definition(&definition)?;
        if self
            .tools
            .iter()
            .any(|registered| registered.definition().name == definition.name)
        {
            return Err(ToolRuntimeError::DuplicateToolName(definition.name));
        }
        self.tools.push(Arc::new(tool));
        Ok(())
    }

    pub fn definitions(&self) -> Vec<ToolDefinition> {
        self.tools.iter().map(|tool| tool.definition()).collect()
    }

    pub fn cancel(&mut self) {
        self.cancellation.cancel();
    }

    /// 新 Agent run 开始前重置上一次 run 的取消状态。
    ///
    /// 取消只属于当前执行批次；若令牌永久保持 cancelled，同一 Session 后续 prompt 的所有
    /// 工具都会被错误拒绝。
    pub fn reset_cancellation(&mut self) {
        if self.has_active_batch() {
            return;
        }
        self.cancellation = CancellationToken::default();
    }

    /// 根据工具声明启动一个批次。批次内任一 sequential 工具都会强制串行，以匹配
    /// TypeScript Agent Loop 的全批次降级语义。
    pub fn start(
        &mut self,
        calls: impl IntoIterator<Item = ToolCall>,
        timestamp: u64,
    ) -> Result<Vec<ToolRuntimeEvent>, ToolRuntimeError> {
        if self.has_active_batch() {
            return Err(ToolRuntimeError::BatchAlreadyActive);
        }
        self.reap_workers();
        self.cancellation = CancellationToken::default();
        self.pending = calls.into_iter().collect();
        self.result_order = self
            .pending
            .iter()
            .map(|call| call.tool_call_id.clone())
            .collect();
        self.completed.clear();
        self.serial_batch = self.pending.iter().any(|call| {
            self.tools.iter().any(|tool| {
                tool.definition().name == call.tool_name
                    && tool.definition().execution_mode == protocol::ToolExecutionMode::Sequential
            })
        });
        self.batch_timestamp = timestamp;
        let mut events = Vec::new();
        if self.serial_batch {
            self.start_next(&mut events);
        } else {
            while !self.pending.is_empty() {
                self.start_next(&mut events);
            }
        }
        Ok(events)
    }

    /// 保留显式串行入口，供不包含执行模式的旧嵌入方使用。
    pub fn start_serial(
        &mut self,
        calls: impl IntoIterator<Item = ToolCall>,
        timestamp: u64,
    ) -> Result<Vec<ToolRuntimeEvent>, ToolRuntimeError> {
        self.start_with_mode(calls, timestamp, true)
    }

    fn start_with_mode(
        &mut self,
        calls: impl IntoIterator<Item = ToolCall>,
        timestamp: u64,
        serial_batch: bool,
    ) -> Result<Vec<ToolRuntimeEvent>, ToolRuntimeError> {
        if self.has_active_batch() {
            return Err(ToolRuntimeError::BatchAlreadyActive);
        }
        self.reap_workers();
        self.cancellation = CancellationToken::default();
        self.pending = calls.into_iter().collect();
        self.result_order = self
            .pending
            .iter()
            .map(|call| call.tool_call_id.clone())
            .collect();
        self.completed.clear();
        self.serial_batch = serial_batch;
        self.batch_timestamp = timestamp;
        let mut events = Vec::new();
        self.start_next(&mut events);
        Ok(events)
    }

    pub fn poll(&mut self, _timestamp: u64) -> Vec<ToolRuntimeEvent> {
        self.reap_workers();
        let mut events = Vec::new();
        let mut index = 0;
        while index < self.active.len() {
            let forced_error = if self.cancellation.is_cancelled() {
                Some("Tool execution aborted")
            } else if self.active[index]
                .deadline
                .is_some_and(|deadline| Instant::now() >= deadline)
            {
                Some("Tool execution timed out")
            } else {
                None
            };
            if let Some(message) = forced_error {
                self.finish_active_at(index, Err(ToolExecutionError::new(message)));
                continue;
            }
            match self.active[index].receiver.try_recv() {
                Ok(WorkerEvent::Updated {
                    tool_call_id,
                    output,
                }) => {
                    events.push(ToolRuntimeEvent::Updated {
                        tool_call_id,
                        content: output.content,
                        details: output.details,
                    });
                    index += 1;
                }
                Ok(WorkerEvent::Finished(output)) => self.finish_active_at(index, output),
                Err(TryRecvError::Empty) => index += 1,
                Err(TryRecvError::Disconnected) => self.finish_active_at(
                    index,
                    Err(ToolExecutionError::new(
                        "Tool worker stopped without a result",
                    )),
                ),
            }
        }
        if self.cancellation.is_cancelled() {
            // Abort 不启动尚未执行的调用，避免副作用继续发生。
            self.pending.clear();
        } else if self.serial_batch && self.active.is_empty() {
            self.start_next(&mut events);
        }
        self.emit_stable_results(&mut events);
        events
    }

    pub fn has_active_batch(&self) -> bool {
        !self.active.is_empty() || !self.pending.is_empty()
    }

    fn start_next(&mut self, events: &mut Vec<ToolRuntimeEvent>) {
        let Some(call) = self.pending.pop_front() else {
            return;
        };
        events.push(ToolRuntimeEvent::Started {
            tool_call: call.clone(),
        });
        let deadline = self
            .timeout
            .and_then(|duration| Instant::now().checked_add(duration));
        let cancellation = self.cancellation.clone();
        let tool = self
            .tools
            .iter()
            .find(|tool| tool.definition().name == call.tool_name)
            .cloned();
        let validation = tool
            .as_ref()
            .and_then(|tool| validate_input(&tool.definition().input_schema, &call.input).err());
        let (sender, receiver) = mpsc::channel();
        let worker_call = call.clone();
        let worker = thread::spawn(move || {
            let output = match (tool, validation) {
                (None, _) => Err(ToolExecutionError::new(format!(
                    "Tool {} not found",
                    worker_call.tool_name
                ))),
                (Some(_), Some(message)) => Err(ToolExecutionError::new(message)),
                (Some(tool), None) => {
                    let context = ToolExecutionContext::new(
                        cancellation,
                        deadline,
                        sender.clone(),
                        worker_call.tool_call_id.clone(),
                    );
                    context
                        .check_active()
                        .and_then(|()| tool.execute(&worker_call, &context))
                }
            };
            let _ = sender.send(WorkerEvent::Finished(output));
        });
        self.active.push(ActiveExecution {
            call,
            timestamp: self.batch_timestamp,
            deadline,
            receiver,
            worker,
        });
    }

    fn finish_active_at(&mut self, index: usize, output: Result<ToolOutput, ToolExecutionError>) {
        let execution = self.active.swap_remove(index);
        let result = match output {
            Ok(output) => ToolResult {
                tool_call_id: execution.call.tool_call_id.clone(),
                tool_name: execution.call.tool_name.clone(),
                input: execution.call.input.clone(),
                content: output.content,
                details: output.details,
                is_error: false,
                timestamp: execution.timestamp,
            },
            Err(error) => error_result(&execution.call, execution.timestamp, error.message()),
        };
        self.retired_workers.push(execution.worker);
        self.completed.insert(result.tool_call_id.clone(), result);
    }

    fn emit_stable_results(&mut self, events: &mut Vec<ToolRuntimeEvent>) {
        while let Some(tool_call_id) = self.result_order.front().cloned() {
            let Some(result) = self.completed.remove(&tool_call_id) else {
                break;
            };
            self.result_order.pop_front();
            events.push(ToolRuntimeEvent::Finished { result });
        }
    }

    fn reap_workers(&mut self) {
        let mut index = 0;
        while index < self.retired_workers.len() {
            if self.retired_workers[index].is_finished() {
                let worker = self.retired_workers.swap_remove(index);
                let _ = worker.join();
            } else {
                index += 1;
            }
        }
    }
}

impl ToolRuntimePort for ToolRuntime {
    fn start(
        &mut self,
        calls: Vec<ToolCall>,
        timestamp: u64,
    ) -> Result<Vec<ToolRuntimePortEvent>, String> {
        ToolRuntime::start(self, calls, timestamp)
            .map(|events| events.into_iter().map(port_event).collect())
            .map_err(|error| format!("Tool Runtime 无法启动批次：{error:?}"))
    }

    fn poll(&mut self, timestamp: u64) -> Vec<ToolRuntimePortEvent> {
        ToolRuntime::poll(self, timestamp)
            .into_iter()
            .map(port_event)
            .collect()
    }

    fn cancel(&mut self) {
        ToolRuntime::cancel(self);
    }

    fn has_active_batch(&self) -> bool {
        ToolRuntime::has_active_batch(self)
    }
}

fn port_event(event: ToolRuntimeEvent) -> ToolRuntimePortEvent {
    match event {
        ToolRuntimeEvent::Started { tool_call } => ToolRuntimePortEvent::Started { tool_call },
        ToolRuntimeEvent::Updated {
            tool_call_id,
            content,
            details,
        } => ToolRuntimePortEvent::Updated {
            tool_call_id,
            content,
            details,
        },
        ToolRuntimeEvent::Finished { result } => ToolRuntimePortEvent::Finished { result },
    }
}

impl Default for ToolRuntime {
    fn default() -> Self {
        Self::new()
    }
}

fn validate_definition(definition: &ToolDefinition) -> Result<(), ToolRuntimeError> {
    if definition.input_schema.get("type").and_then(Value::as_str) != Some("object") {
        return Err(ToolRuntimeError::InvalidToolSchema(definition.name.clone()));
    }
    Ok(())
}

/// 校验生产内置工具使用的 JSON Schema 子集：object、required、properties、
/// additionalProperties 及 string/number/integer/boolean。未知 schema 关键字由具体工具继续校验。
fn validate_input(schema: &Value, input: &Value) -> Result<(), String> {
    let object = input
        .as_object()
        .ok_or_else(|| "Tool input must be a JSON object".to_owned())?;
    let required = schema
        .get("required")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .collect::<BTreeSet<_>>();
    for property in required {
        if !object.contains_key(property) {
            return Err(format!(
                "Tool input is missing required property: {property}"
            ));
        }
    }
    let properties = schema.get("properties").and_then(Value::as_object);
    if schema.get("additionalProperties") == Some(&Value::Bool(false)) {
        for key in object.keys() {
            if !properties.is_some_and(|properties| properties.contains_key(key)) {
                return Err(format!("Tool input contains unknown property: {key}"));
            }
        }
    }
    if let Some(properties) = properties {
        for (name, value) in object {
            let Some(property) = properties.get(name) else {
                continue;
            };
            let valid = match property.get("type").and_then(Value::as_str) {
                Some("string") => value.is_string(),
                Some("number") => value.is_number(),
                Some("integer") => value.as_i64().is_some() || value.as_u64().is_some(),
                Some("boolean") => value.is_boolean(),
                _ => true,
            };
            if !valid {
                return Err(format!("Tool input property has invalid type: {name}"));
            }
        }
    }
    Ok(())
}

fn error_result(call: &ToolCall, timestamp: u64, message: impl Into<String>) -> ToolResult {
    ToolResult {
        tool_call_id: call.tool_call_id.clone(),
        tool_name: call.tool_name.clone(),
        input: call.input.clone(),
        content: vec![TextOrImageContent::Text {
            text: message.into(),
        }],
        details: None,
        is_error: true,
        timestamp,
    }
}

#[cfg(test)]
mod tests {
    use std::{
        sync::{
            Arc,
            atomic::{AtomicBool, Ordering},
        },
        thread,
        time::{Duration, Instant},
    };

    use protocol::{TextOrImageContent, ToolCall, ToolDefinition, ToolExecutionMode};
    use serde_json::json;

    use super::{
        Tool, ToolExecutionContext, ToolExecutionError, ToolOutput, ToolRuntime, ToolRuntimeError,
        ToolRuntimeEvent,
    };

    struct EchoTool;

    impl Tool for EchoTool {
        fn definition(&self) -> ToolDefinition {
            ToolDefinition {
                name: "echo".to_owned(),
                description: "Echo input".to_owned(),
                input_schema: json!({
                    "type": "object",
                    "properties": { "value": { "type": "string" } },
                    "required": ["value"],
                    "additionalProperties": false
                }),
                execution_mode: protocol::ToolExecutionMode::Parallel,
            }
        }

        fn execute(
            &self,
            call: &ToolCall,
            context: &ToolExecutionContext,
        ) -> Result<ToolOutput, ToolExecutionError> {
            context.check_active()?;
            Ok(ToolOutput::text(format!("echo: {}", call.input)))
        }
    }

    struct ControlledTool {
        release: Arc<AtomicBool>,
    }

    /// 第二个调用先完成、首个调用由测试显式放行的工具。
    ///
    /// 它将并发时的物理完成顺序固定为 call-2 → call-1，从而验证 Runtime 不会把 worker
    /// 调度顺序泄漏到 Provider continuation transcript。
    struct ReverseCompletionTool {
        release_first: Arc<AtomicBool>,
        second_started: Arc<AtomicBool>,
    }

    impl Tool for ReverseCompletionTool {
        fn definition(&self) -> ToolDefinition {
            ToolDefinition {
                name: "reverse".to_owned(),
                description: "按测试指定的逆序完成工具".to_owned(),
                input_schema: json!({
                    "type": "object",
                    "properties": { "order": { "type": "number" } },
                    "required": ["order"],
                    "additionalProperties": false
                }),
                execution_mode: ToolExecutionMode::Parallel,
            }
        }

        fn execute(
            &self,
            call: &ToolCall,
            _context: &ToolExecutionContext,
        ) -> Result<ToolOutput, ToolExecutionError> {
            let order = call.input["order"]
                .as_u64()
                .ok_or_else(|| ToolExecutionError::new("missing order"))?;
            if order == 1 {
                while !self.release_first.load(Ordering::Acquire) {
                    thread::yield_now();
                }
            } else {
                self.second_started.store(true, Ordering::Release);
            }
            Ok(ToolOutput::text(format!("finished-{order}")))
        }
    }

    struct SequentialEchoTool;

    impl Tool for SequentialEchoTool {
        fn definition(&self) -> ToolDefinition {
            ToolDefinition {
                name: "sequential-echo".to_owned(),
                description: "强制整批串行的测试工具".to_owned(),
                input_schema: json!({
                    "type": "object",
                    "properties": {},
                    "additionalProperties": false
                }),
                execution_mode: ToolExecutionMode::Sequential,
            }
        }

        fn execute(
            &self,
            _call: &ToolCall,
            _context: &ToolExecutionContext,
        ) -> Result<ToolOutput, ToolExecutionError> {
            Ok(ToolOutput::text("sequential"))
        }
    }

    impl Tool for ControlledTool {
        fn definition(&self) -> ToolDefinition {
            ToolDefinition {
                name: "controlled".to_owned(),
                description: "可控制完成时机的测试工具".to_owned(),
                input_schema: json!({
                    "type": "object",
                    "properties": {},
                    "additionalProperties": false
                }),
                execution_mode: protocol::ToolExecutionMode::Parallel,
            }
        }

        fn execute(
            &self,
            _call: &ToolCall,
            context: &ToolExecutionContext,
        ) -> Result<ToolOutput, ToolExecutionError> {
            context.report_update(ToolOutput::text("working"));
            // 故意不检查取消，模拟无法协作取消的第三方阻塞工具。Runtime 必须仍能按时产生
            // 逻辑终态，并隔离 release 后返回的迟到结果。
            while !self.release.load(Ordering::Acquire) {
                thread::sleep(Duration::from_millis(1));
            }
            context.report_update(ToolOutput::text("late update"));
            Ok(ToolOutput::text("late result"))
        }
    }

    fn run_to_completion(
        runtime: &mut ToolRuntime,
        calls: Vec<ToolCall>,
        timestamp: u64,
    ) -> Vec<ToolRuntimeEvent> {
        let mut events = runtime.start_serial(calls, timestamp).unwrap();
        while runtime.has_active_batch() {
            events.extend(runtime.poll(timestamp));
            std::thread::yield_now();
        }
        events
    }

    #[test]
    fn owns_definitions_and_executes_calls_in_source_order() {
        let mut runtime = ToolRuntime::new();
        runtime.register(EchoTool).unwrap();
        assert_eq!(runtime.definitions()[0].name, "echo");

        let events = run_to_completion(
            &mut runtime,
            vec![
                ToolCall {
                    tool_call_id: "1".into(),
                    tool_name: "echo".into(),
                    input: json!({"value":"hello"}),
                },
                ToolCall {
                    tool_call_id: "2".into(),
                    tool_name: "missing".into(),
                    input: json!({}),
                },
            ],
            9,
        );
        assert!(matches!(events[0], ToolRuntimeEvent::Started { .. }));
        let results = events
            .iter()
            .filter_map(|event| match event {
                ToolRuntimeEvent::Finished { result } => Some(result),
                ToolRuntimeEvent::Started { .. } | ToolRuntimeEvent::Updated { .. } => None,
            })
            .collect::<Vec<_>>();
        assert!(!results[0].is_error);
        assert_eq!(
            results[1].content,
            vec![TextOrImageContent::Text {
                text: "Tool missing not found".into()
            }]
        );
    }

    #[test]
    fn validates_arguments_and_resets_cancellation_between_runs() {
        let mut runtime = ToolRuntime::new();
        runtime.register(EchoTool).unwrap();
        assert_eq!(
            runtime.register(EchoTool),
            Err(ToolRuntimeError::DuplicateToolName("echo".into()))
        );
        let invalid = run_to_completion(
            &mut runtime,
            vec![ToolCall {
                tool_call_id: "1".into(),
                tool_name: "echo".into(),
                input: json!({"value": 1}),
            }],
            1,
        );
        let result = invalid
            .iter()
            .find_map(|event| match event {
                ToolRuntimeEvent::Finished { result } => Some(result),
                ToolRuntimeEvent::Started { .. } | ToolRuntimeEvent::Updated { .. } => None,
            })
            .unwrap();
        assert!(result.is_error);
        assert_eq!(
            result.content,
            vec![TextOrImageContent::Text {
                text: "Tool input property has invalid type: value".into()
            }]
        );

        runtime.cancel();
        runtime.reset_cancellation();
        let next_run = run_to_completion(
            &mut runtime,
            vec![ToolCall {
                tool_call_id: "3".into(),
                tool_name: "echo".into(),
                input: json!({"value":"next run"}),
            }],
            3,
        );
        let result = next_run
            .iter()
            .find_map(|event| match event {
                ToolRuntimeEvent::Finished { result } => Some(result),
                ToolRuntimeEvent::Started { .. } | ToolRuntimeEvent::Updated { .. } => None,
            })
            .unwrap();
        assert!(!result.is_error);
        assert_eq!(
            result.content,
            vec![TextOrImageContent::Text {
                text: "echo: {\"value\":\"next run\"}".into()
            }]
        );
    }

    #[test]
    fn parallel_batch_starts_all_calls_and_emits_reverse_completions_in_source_order() {
        let release_first = Arc::new(AtomicBool::new(false));
        let second_started = Arc::new(AtomicBool::new(false));
        let mut runtime = ToolRuntime::new().with_timeout(None);
        runtime
            .register(ReverseCompletionTool {
                release_first: Arc::clone(&release_first),
                second_started: Arc::clone(&second_started),
            })
            .unwrap();

        let mut events = runtime
            .start(
                [
                    ToolCall {
                        tool_call_id: "call-1".into(),
                        tool_name: "reverse".into(),
                        input: json!({ "order": 1 }),
                    },
                    ToolCall {
                        tool_call_id: "call-2".into(),
                        tool_name: "reverse".into(),
                        input: json!({ "order": 2 }),
                    },
                ],
                10,
            )
            .unwrap();
        assert_eq!(
            events
                .iter()
                .filter_map(|event| match event {
                    ToolRuntimeEvent::Started { tool_call } =>
                        Some(tool_call.tool_call_id.as_str()),
                    ToolRuntimeEvent::Updated { .. } | ToolRuntimeEvent::Finished { .. } => None,
                })
                .collect::<Vec<_>>(),
            vec!["call-1", "call-2"]
        );

        let deadline = Instant::now() + Duration::from_secs(1);
        while !second_started.load(Ordering::Acquire) {
            events.extend(runtime.poll(11));
            assert!(
                Instant::now() < deadline,
                "second parallel worker should start"
            );
            thread::yield_now();
        }
        let deadline = Instant::now() + Duration::from_secs(1);
        while runtime.completed.get("call-2").is_none() {
            events.extend(runtime.poll(12));
            assert!(
                Instant::now() < deadline,
                "second worker should finish first"
            );
            thread::yield_now();
        }
        assert!(events.iter().all(|event| !matches!(
            event,
            ToolRuntimeEvent::Finished { result } if result.tool_call_id == "call-2"
        )));

        release_first.store(true, Ordering::Release);
        while runtime.has_active_batch() {
            events.extend(runtime.poll(13));
            thread::yield_now();
        }
        assert_eq!(
            events
                .iter()
                .filter_map(|event| match event {
                    ToolRuntimeEvent::Finished { result } => Some(result.tool_call_id.as_str()),
                    ToolRuntimeEvent::Started { .. } | ToolRuntimeEvent::Updated { .. } => None,
                })
                .collect::<Vec<_>>(),
            vec!["call-1", "call-2"]
        );
    }

    #[test]
    fn sequential_tool_forces_the_whole_batch_to_start_one_call_at_a_time() {
        let release = Arc::new(AtomicBool::new(false));
        let mut runtime = ToolRuntime::new().with_timeout(None);
        runtime
            .register(ControlledTool {
                release: Arc::clone(&release),
            })
            .unwrap();
        runtime.register(SequentialEchoTool).unwrap();

        let events = runtime
            .start(
                [
                    ToolCall {
                        tool_call_id: "slow-first".into(),
                        tool_name: "controlled".into(),
                        input: json!({}),
                    },
                    ToolCall {
                        tool_call_id: "sequential-second".into(),
                        tool_name: "sequential-echo".into(),
                        input: json!({}),
                    },
                ],
                20,
            )
            .unwrap();
        assert!(matches!(
            events.as_slice(),
            [ToolRuntimeEvent::Started { tool_call }] if tool_call.tool_call_id == "slow-first"
        ));

        release.store(true, Ordering::Release);
        let deadline = Instant::now() + Duration::from_secs(1);
        let mut observed = events;
        while runtime.has_active_batch() {
            observed.extend(runtime.poll(21));
            assert!(Instant::now() < deadline, "serial batch should finish");
            thread::yield_now();
        }
        assert_eq!(
            observed
                .iter()
                .filter_map(|event| match event {
                    ToolRuntimeEvent::Started { tool_call } =>
                        Some(tool_call.tool_call_id.as_str()),
                    ToolRuntimeEvent::Updated { .. } | ToolRuntimeEvent::Finished { .. } => None,
                })
                .collect::<Vec<_>>(),
            vec!["slow-first", "sequential-second"]
        );
    }

    #[test]
    fn reports_progress_without_blocking_the_session_thread() {
        let release = Arc::new(AtomicBool::new(false));
        let mut runtime = ToolRuntime::new().with_timeout(None);
        runtime
            .register(ControlledTool {
                release: Arc::clone(&release),
            })
            .unwrap();

        let started_at = Instant::now();
        let events = runtime
            .start_serial(
                [ToolCall {
                    tool_call_id: "slow-1".into(),
                    tool_name: "controlled".into(),
                    input: json!({}),
                }],
                10,
            )
            .unwrap();
        assert!(started_at.elapsed() < Duration::from_millis(50));
        assert!(matches!(
            events.as_slice(),
            [ToolRuntimeEvent::Started { .. }]
        ));

        let deadline = Instant::now() + Duration::from_secs(1);
        let update = loop {
            if let Some(update) = runtime
                .poll(11)
                .into_iter()
                .find(|event| matches!(event, ToolRuntimeEvent::Updated { .. }))
            {
                break update;
            }
            assert!(Instant::now() < deadline, "progress update should arrive");
            thread::yield_now();
        };
        assert!(matches!(
            update,
            ToolRuntimeEvent::Updated { content, .. }
                if content == vec![TextOrImageContent::Text { text: "working".into() }]
        ));
        release.store(true, Ordering::Release);
        let deadline = Instant::now() + Duration::from_secs(1);
        while runtime.has_active_batch() {
            let _ = runtime.poll(12);
            assert!(Instant::now() < deadline, "controlled tool should finish");
            thread::yield_now();
        }
    }

    #[test]
    fn timeout_has_one_terminal_and_drops_late_updates_and_output() {
        let release = Arc::new(AtomicBool::new(false));
        let mut runtime = ToolRuntime::new().with_timeout(Some(Duration::from_millis(10)));
        runtime
            .register(ControlledTool {
                release: Arc::clone(&release),
            })
            .unwrap();
        let mut events = runtime
            .start_serial(
                [ToolCall {
                    tool_call_id: "slow-timeout".into(),
                    tool_name: "controlled".into(),
                    input: json!({}),
                }],
                20,
            )
            .unwrap();
        thread::sleep(Duration::from_millis(20));
        events.extend(runtime.poll(21));
        assert!(!runtime.has_active_batch());
        assert_eq!(
            events
                .iter()
                .filter(|event| matches!(event, ToolRuntimeEvent::Finished { .. }))
                .count(),
            1
        );
        assert!(events.iter().any(|event| matches!(
            event,
            ToolRuntimeEvent::Finished { result }
                if result.is_error
                    && matches!(result.content.as_slice(), [TextOrImageContent::Text { text }]
                        if text == "Tool execution timed out")
        )));

        release.store(true, Ordering::Release);
        thread::sleep(Duration::from_millis(10));
        assert!(
            runtime.poll(22).is_empty(),
            "late worker output must be isolated"
        );
        assert!(
            runtime.retired_workers.is_empty(),
            "naturally returned timed-out workers must be joined"
        );
    }

    #[test]
    fn cancel_settles_active_skips_pending_calls_then_allows_a_clean_next_batch() {
        let release = Arc::new(AtomicBool::new(false));
        let mut runtime = ToolRuntime::new().with_timeout(None);
        runtime
            .register(ControlledTool {
                release: Arc::clone(&release),
            })
            .unwrap();
        runtime.register(EchoTool).unwrap();
        let mut events = runtime
            .start_serial(
                [
                    ToolCall {
                        tool_call_id: "active".into(),
                        tool_name: "controlled".into(),
                        input: json!({}),
                    },
                    ToolCall {
                        tool_call_id: "pending".into(),
                        tool_name: "echo".into(),
                        input: json!({"value": "never runs"}),
                    },
                ],
                30,
            )
            .unwrap();
        runtime.cancel();
        events.extend(runtime.poll(31));
        assert!(!runtime.has_active_batch());
        let results = events
            .iter()
            .filter_map(|event| match event {
                ToolRuntimeEvent::Finished { result } => Some(result),
                ToolRuntimeEvent::Started { .. } | ToolRuntimeEvent::Updated { .. } => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].tool_call_id, "active");
        assert!(results.iter().all(|result| result.is_error));
        assert!(results.iter().all(|result| matches!(
            result.content.as_slice(),
            [TextOrImageContent::Text { text }] if text == "Tool execution aborted"
        )));

        let next = run_to_completion(
            &mut runtime,
            vec![ToolCall {
                tool_call_id: "next".into(),
                tool_name: "echo".into(),
                input: json!({"value": "clean"}),
            }],
            32,
        );
        assert!(next.iter().any(|event| matches!(
            event,
            ToolRuntimeEvent::Finished { result } if !result.is_error
        )));
        release.store(true, Ordering::Release);
        thread::sleep(Duration::from_millis(10));
        assert!(runtime.poll(33).is_empty());
        assert!(
            runtime.retired_workers.is_empty(),
            "naturally returned cancelled workers must be joined"
        );
    }
}

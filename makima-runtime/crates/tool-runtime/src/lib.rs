//! Rust Core 的最小 Tool Runtime。
//!
//! Agent Loop 只负责决定何时执行工具；本 crate 负责按调用顺序定位工具、执行并把成功或
//! 失败归一化为共享 [`protocol::ToolResult`]。它不依赖 Provider SDK、Session Store、TUI
//! 或具体 Sandbox 后端，因此命令工具可在外层通过一个 [`Tool`] adapter 接入。

use agent_loop::{ToolRuntimePort, ToolRuntimePortEvent};
use protocol::{TextOrImageContent, ToolCall, ToolResult};

/// 工具执行过程中可观察的稳定生命周期事件。
#[derive(Debug, Clone, PartialEq)]
pub enum ToolRuntimeEvent {
    /// 已接收调用，尚未进入工具实现。
    Started { tool_call: ToolCall },
    /// 工具执行完成；业务失败同样通过结果表达，不中断后续串行调用。
    Finished { result: ToolResult },
}

/// 单个工具的实现边界。
///
/// 工具实现可以在内部调用 Sandbox、文件系统或扩展宿主，但不能把这些依赖泄漏给
/// [`ToolRuntime`]。所有错误必须转换为可展示的稳定文本。
pub trait Tool {
    /// Provider 可见的唯一工具名。
    fn name(&self) -> &str;

    /// 执行一次已经完成参数解析的工具调用。
    fn execute(&self, call: &ToolCall) -> Result<ToolOutput, ToolExecutionError>;
}

/// 成功工具调用产生的内容与可选 JSON 详情。
#[derive(Debug, Clone, PartialEq)]
pub struct ToolOutput {
    pub content: Vec<TextOrImageContent>,
    pub details: Option<serde_json::Value>,
}

impl ToolOutput {
    /// 创建纯文本工具输出。
    pub fn text(text: impl Into<String>) -> Self {
        Self {
            content: vec![TextOrImageContent::Text { text: text.into() }],
            details: None,
        }
    }
}

/// 工具执行失败的稳定错误。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolExecutionError {
    message: String,
}

impl ToolExecutionError {
    /// 创建不包含后端内部细节的执行错误。
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }

    /// 返回可安全写入 Tool Result 的错误文本。
    pub fn message(&self) -> &str {
        &self.message
    }
}

/// 注册或调用参数不合法时返回的错误。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ToolRuntimeError {
    DuplicateToolName(String),
    EmptyToolName,
}

/// 串行 Tool Runtime。
///
/// 保持注册顺序和调用顺序；未找到的工具也被转成 `is_error: true` 结果，确保 Agent Loop
/// 可将其作为正常轨迹写入下一轮 Provider 请求，而不是因一个调用中断整个回合。
pub struct ToolRuntime<'a> {
    tools: Vec<&'a dyn Tool>,
}

impl<'a> ToolRuntime<'a> {
    /// 创建空工具注册表。
    pub fn new() -> Self {
        Self { tools: Vec::new() }
    }

    /// 注册一个名称唯一的工具。
    pub fn register(&mut self, tool: &'a dyn Tool) -> Result<(), ToolRuntimeError> {
        let name = tool.name();
        if name.is_empty() {
            return Err(ToolRuntimeError::EmptyToolName);
        }
        if self
            .tools
            .iter()
            .any(|registered| registered.name() == name)
        {
            return Err(ToolRuntimeError::DuplicateToolName(name.to_owned()));
        }
        self.tools.push(tool);
        Ok(())
    }

    /// 依次执行所有工具调用，并为每一个调用发出 start / end 事件。
    pub fn execute_serial(
        &self,
        calls: impl IntoIterator<Item = ToolCall>,
        timestamp: u64,
    ) -> Vec<ToolRuntimeEvent> {
        let mut events = Vec::new();
        for call in calls {
            events.push(ToolRuntimeEvent::Started {
                tool_call: call.clone(),
            });
            let result = match self.tools.iter().find(|tool| tool.name() == call.tool_name) {
                Some(tool) => match tool.execute(&call) {
                    Ok(output) => ToolResult {
                        tool_call_id: call.tool_call_id.clone(),
                        tool_name: call.tool_name.clone(),
                        input: call.input.clone(),
                        content: output.content,
                        details: output.details,
                        is_error: false,
                        timestamp,
                    },
                    Err(error) => error_result(&call, timestamp, error.message()),
                },
                None => error_result(
                    &call,
                    timestamp,
                    format!("Unknown tool: {}", call.tool_name),
                ),
            };
            events.push(ToolRuntimeEvent::Finished { result });
        }
        events
    }
}

impl ToolRuntimePort for ToolRuntime<'_> {
    fn execute_serial(&self, calls: Vec<ToolCall>, timestamp: u64) -> Vec<ToolRuntimePortEvent> {
        ToolRuntime::execute_serial(self, calls, timestamp)
            .into_iter()
            .map(|event| match event {
                ToolRuntimeEvent::Started { tool_call } => {
                    ToolRuntimePortEvent::Started { tool_call }
                }
                ToolRuntimeEvent::Finished { result } => ToolRuntimePortEvent::Finished { result },
            })
            .collect()
    }
}

impl Default for ToolRuntime<'_> {
    fn default() -> Self {
        Self::new()
    }
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
    use protocol::{TextOrImageContent, ToolCall};
    use serde_json::json;

    use super::{
        Tool, ToolExecutionError, ToolOutput, ToolRuntime, ToolRuntimeError, ToolRuntimeEvent,
    };

    struct EchoTool;

    impl Tool for EchoTool {
        fn name(&self) -> &str {
            "echo"
        }

        fn execute(&self, call: &ToolCall) -> Result<ToolOutput, ToolExecutionError> {
            Ok(ToolOutput::text(format!("echo: {}", call.input)))
        }
    }

    struct FailingTool;

    impl Tool for FailingTool {
        fn name(&self) -> &str {
            "fail"
        }

        fn execute(&self, _call: &ToolCall) -> Result<ToolOutput, ToolExecutionError> {
            Err(ToolExecutionError::new("command failed"))
        }
    }

    #[test]
    fn executes_registered_and_unknown_tools_in_source_order() {
        let echo = EchoTool;
        let mut runtime = ToolRuntime::new();
        runtime.register(&echo).expect("tool should register");

        let events = runtime.execute_serial(
            [
                ToolCall {
                    tool_call_id: "call-1".to_owned(),
                    tool_name: "echo".to_owned(),
                    input: json!({ "value": "hello" }),
                },
                ToolCall {
                    tool_call_id: "call-2".to_owned(),
                    tool_name: "missing".to_owned(),
                    input: json!({}),
                },
            ],
            9,
        );

        assert!(matches!(events[0], ToolRuntimeEvent::Started { .. }));
        assert!(matches!(events[2], ToolRuntimeEvent::Started { .. }));
        let ToolRuntimeEvent::Finished { result } = &events[1] else {
            panic!("second event should finish the first call")
        };
        assert!(!result.is_error);
        assert_eq!(result.timestamp, 9);
        assert_eq!(
            result.content,
            vec![TextOrImageContent::Text {
                text: "echo: {\"value\":\"hello\"}".to_owned(),
            }]
        );
        let ToolRuntimeEvent::Finished { result } = &events[3] else {
            panic!("fourth event should finish the second call")
        };
        assert!(result.is_error);
        assert_eq!(
            result.content,
            vec![TextOrImageContent::Text {
                text: "Unknown tool: missing".to_owned()
            }]
        );
    }

    #[test]
    fn converts_tool_failures_to_error_results_and_rejects_duplicate_names() {
        let echo = EchoTool;
        let failing = FailingTool;
        let mut runtime = ToolRuntime::new();
        runtime.register(&echo).unwrap();
        assert_eq!(
            runtime.register(&echo),
            Err(ToolRuntimeError::DuplicateToolName("echo".to_owned()))
        );
        runtime.register(&failing).unwrap();

        let events = runtime.execute_serial(
            [ToolCall {
                tool_call_id: "call-1".to_owned(),
                tool_name: "fail".to_owned(),
                input: json!({}),
            }],
            10,
        );
        let ToolRuntimeEvent::Finished { result } = &events[1] else {
            panic!("second event should finish the call")
        };
        assert!(result.is_error);
        assert_eq!(
            result.content,
            vec![TextOrImageContent::Text {
                text: "command failed".to_owned()
            }]
        );
    }
}

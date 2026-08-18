//! Provider 请求上下文的可替换转换端口。
//!
//! Agent Loop 只维护权威 transcript 及其稳定提交顺序；Provider SDK 所需的裁剪、投影或
//! 模型选择属于运行时边界。本模块将该边界显式化，避免 [`crate::provider_runtime`] 直接把
//! Agent Loop 的内部消息列表耦合为 Host 请求。
//!
//! 该同步端口对应 TypeScript Agent Loop 在每次请求前的 `transformContext` 与
//! `convertToLlm` 两个阶段。TypeScript 的 `prepareNextTurn` 会在工具回填后更新下一轮
//! context；Rust 当前将其统一为下一次 Provider 请求的独立投影，因而不修改持久化的
//! AgentSession 状态，也不会隐式改变用户通过命令选定的模型。

use protocol::{ModelRef, ToolDefinition, TranscriptItem};

/// 当前 Provider 请求相对于 Agent Loop 生命周期的原因。
///
/// 转换器可据此采用不同的压缩策略；例如 continuation 可保留最新工具结果，而 retry
/// 必须继续忽略已从工作 transcript 移除的失败 assistant。该枚举不参与线协议，仅用于
/// Rust Runtime 内的策略选择和可回放测试。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProviderRequestPurpose {
    /// 用户 prompt 被 AgentSession 接受后的第一条请求。
    Initial,
    /// 工具结果、steer 或 follow-up 已稳定提交后的下一轮请求。
    Continuation,
    /// 可重试错误完成退避后，对同一工作上下文发起的请求。
    Retry,
}

/// 传给上下文转换器的不可变 Provider 请求快照。
///
/// 所有字段都是借用的只读视图。转换器若要裁剪或改写内容，必须在输出中创建新的值，不能
/// 修改 Agent Loop 的权威 transcript；因此转换失败也不会污染后续 retry 或 continuation。
#[derive(Debug, Clone, Copy)]
pub struct ProviderContextInput<'a> {
    pub request_id: &'a str,
    pub purpose: ProviderRequestPurpose,
    pub timestamp: u64,
    pub model: &'a ModelRef,
    pub system_prompt: &'a str,
    pub messages: &'a [TranscriptItem],
    pub tools: &'a [ToolDefinition],
}

/// 转换完成后用于构造 Provider Request 的独立上下文。
///
/// 显式拥有全部字段使转换器能对齐 TypeScript 的 context 投影能力，包括临时的模型覆盖。
/// 覆盖仅影响当前请求，绝不写回 AgentSession；持久化模型变更仍必须通过 `set_model` 命令。
#[derive(Debug, Clone, PartialEq)]
pub struct ProviderContext {
    pub model: ModelRef,
    pub system_prompt: String,
    pub messages: Vec<TranscriptItem>,
    pub tools: Vec<ToolDefinition>,
}

/// Provider 请求前的上下文转换端口。
///
/// 实现可保存少量策略状态，故方法接收 `&mut self`。运行时为每条请求调用一次；初始请求、
/// 工具 continuation 与 retry 都复用同一入口，避免三条路径出现裁剪规则漂移。
pub trait ContextTransformationPort: Send {
    fn transform(&mut self, input: ProviderContextInput<'_>) -> Result<ProviderContext, String>;
}

/// 保持既有行为的默认转换器。
///
/// 它完整复制 Agent Loop 已稳定提交的 transcript、工具声明和 session 系统提示，等价于
/// 重构前 `ProviderStreamDriver` 直接构造 `ProviderRequest` 的行为。
#[derive(Debug, Default, Clone, Copy)]
pub struct IdentityContextTransformer;

impl ContextTransformationPort for IdentityContextTransformer {
    fn transform(&mut self, input: ProviderContextInput<'_>) -> Result<ProviderContext, String> {
        Ok(ProviderContext {
            model: input.model.clone(),
            system_prompt: input.system_prompt.to_owned(),
            messages: input.messages.to_vec(),
            tools: input.tools.to_vec(),
        })
    }
}

#[cfg(test)]
mod tests {
    use protocol::{
        ModelRef, TextOrImageContent, ToolDefinition, TranscriptItem, UserRole, UserTranscriptItem,
    };

    use super::{
        ContextTransformationPort, IdentityContextTransformer, ProviderContextInput,
        ProviderRequestPurpose,
    };

    #[test]
    fn identity_transformer_copies_the_complete_provider_context() {
        let model = ModelRef {
            provider: "test".to_owned(),
            id: "model-a".to_owned(),
        };
        let messages = vec![TranscriptItem::User(UserTranscriptItem {
            id: "user-1".to_owned(),
            role: UserRole::User,
            content: vec![TextOrImageContent::Text {
                text: "hello".to_owned(),
            }],
            timestamp: 100,
        })];
        let tools = vec![ToolDefinition {
            name: "echo".to_owned(),
            description: "echoes input".to_owned(),
            input_schema: serde_json::json!({ "type": "object" }),
            execution_mode: Default::default(),
        }];
        let mut transformer = IdentityContextTransformer;

        let output = transformer
            .transform(ProviderContextInput {
                request_id: "session-1-provider-1",
                purpose: ProviderRequestPurpose::Initial,
                timestamp: 100,
                model: &model,
                system_prompt: "system prompt",
                messages: &messages,
                tools: &tools,
            })
            .expect("identity transformation should succeed");

        assert_eq!(output.model, model);
        assert_eq!(output.system_prompt, "system prompt");
        assert_eq!(output.messages, messages);
        assert_eq!(output.tools, tools);
    }
}

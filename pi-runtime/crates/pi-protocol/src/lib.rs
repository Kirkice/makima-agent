//! Pi Agent 的跨语言协议模型。
//!
//! 本 crate 只放 Rust 与 TypeScript 之间需要共享的数据结构，不放业务逻辑。
//! 这样可以避免 Rust Core 依赖 TypeScript 的内部类，也避免 TypeScript Host
//! 依赖 Rust 的运行时实现。后续应以 `packages/protocol/src/schemas.ts` 为
//! 协议事实来源，并为两侧补充自动生成或 conformance 校验。

use serde::{Deserialize, Serialize};

/// 当前 Rust/TypeScript 协议版本。
///
/// 版本发生不兼容变化时必须递增。兼容性变化优先通过新增可选字段完成，
/// 不要把运行时内部状态直接暴露到协议中。
pub const PROTOCOL_VERSION: u32 = 1;

/// Agent 当前所处的生命周期阶段。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SessionPhase {
    Idle,
    Turn,
    Compaction,
    BranchSummary,
    Retry,
}

/// 模型的稳定外部引用。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModelRef {
    pub provider: String,
    pub id: String,
}

/// Agent 可以接收的最小命令集合。
///
/// 第一阶段只实现与现有 RPC 协议一致的命令。新增命令时应同时更新
/// TypeScript Schema、Rust 模型和跨语言 fixture。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "command", rename_all = "snake_case")]
pub enum Command {
    List,
    Create {
        cwd: Option<String>,
        name: Option<String>,
        model: Option<ModelRef>,
        thinking_level: Option<String>,
    },
    Attach {
        session_id: String,
    },
    Detach {
        session_id: String,
    },
    Prompt {
        session_id: String,
        text: String,
    },
    Steer {
        session_id: String,
        text: String,
    },
    Abort {
        session_id: String,
    },
    SetModel {
        session_id: String,
        model: ModelRef,
    },
    SetThinking {
        session_id: String,
        thinking_level: String,
    },
}

/// Rust Core 当前对外公布的最小 Session 快照。
///
/// 这里暂不复制完整 transcript 类型，避免在第一阶段把内部消息模型过早
/// 固化。后续会在协议 fixture 稳定后补齐 transcript 和 progress 模型。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionSnapshot {
    pub id: String,
    pub cwd: String,
    pub phase: SessionPhase,
    pub model: ModelRef,
    pub thinking_level: String,
    pub revision: u64,
}

/// 统一的协议错误，供 CLI、RPC Server 和 TypeScript Host 使用。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProtocolError {
    pub code: String,
    pub message: String,
}

#[cfg(test)]
mod tests {
    use super::{Command, ModelRef, PROTOCOL_VERSION};

    #[test]
    fn command_uses_stable_tagged_json_shape() {
        let command = Command::Prompt {
            session_id: "session-1".to_owned(),
            text: "hello".to_owned(),
        };

        let encoded = serde_json::to_value(command).expect("command should serialize");
        assert_eq!(encoded["command"], "prompt");
        assert_eq!(encoded["session_id"], "session-1");
        assert_eq!(encoded["text"], "hello");
    }

    #[test]
    fn protocol_version_is_explicit() {
        assert_eq!(PROTOCOL_VERSION, 1);
        let model = ModelRef {
            provider: "test".to_owned(),
            id: "model".to_owned(),
        };
        assert_eq!(model.provider, "test");
    }
}

//! Pi Agent Rust 核心运行时的第一阶段骨架。
//!
//! 当前只实现协议驱动的 Session 生命周期，不调用 Provider、不执行工具、
//! 不读取 TUI。这样可以先验证 Rust Core 的边界和数据契约，再逐步迁移
//! Agent Loop、Session Store、Tool Runtime 与 Sandbox。

/// AgentSession 领域层及其外部能力端口。
pub mod agent_session;

use protocol::{
    Command, ModelRef, ProtocolError, ProtocolErrorCode, SessionPhase, SessionSnapshot,
    ThinkingLevel,
};
use session::{JsonlSessionStore, SessionStoreError};

/// 一个最小的内存 Session。
///
/// 这是迁移阶段的临时实现，不是最终的持久化方案。它的价值在于提供一个
/// 与 RPC/CLI 对接的稳定最小核心，避免第一步就把数据库、Provider 和 TUI
/// 同时引入，降低回归范围。
#[derive(Debug, Clone)]
pub struct SessionRuntime {
    snapshot: SessionSnapshot,
}

impl SessionRuntime {
    /// 将 Session 的持久化边界暴露给上层编排器。
    ///
    /// 该方法只负责打开 JSONL Store，不会改变当前 SessionRuntime 的内存
    /// 快照，也不会自动接管 prompt。这样可以在迁移期间由 TypeScript 继续
    /// 负责完整 Agent 流程，而 Rust 只逐步接管持久化。
    pub fn open_store(
        path: impl Into<std::path::PathBuf>,
    ) -> Result<JsonlSessionStore, SessionStoreError> {
        JsonlSessionStore::open(path)
    }

    /// 创建一个新的 Session。
    pub fn new(id: impl Into<String>, cwd: impl Into<String>, model: ModelRef) -> Self {
        Self {
            snapshot: SessionSnapshot {
                id: id.into(),
                name: None,
                cwd: cwd.into(),
                created_at: 0,
                updated_at: 0,
                phase: SessionPhase::Idle,
                model,
                thinking_level: ThinkingLevel::Medium,
                attached: false,
                locked: false,
                revision: 0,
                transcript: Vec::new(),
                queued_steer: Vec::new(),
                queued_steer_count: 0,
            },
        }
    }

    /// 返回当前快照的副本，避免调用方直接修改核心状态。
    pub fn snapshot(&self) -> SessionSnapshot {
        self.snapshot.clone()
    }

    /// 执行一个第一阶段支持的命令。
    ///
    /// 目前只实现不会触发模型请求的控制命令。`prompt`、工具调用和持久化
    /// 将在后续迁移阶段实现；提前返回明确错误比静默丢失用户请求更安全。
    pub fn execute(&mut self, command: Command) -> Result<SessionSnapshot, ProtocolError> {
        match command {
            Command::SetModel { model, .. } => {
                self.snapshot.model = model;
                self.snapshot.revision += 1;
                Ok(self.snapshot())
            }
            Command::SetThinking { thinking_level, .. } => {
                self.snapshot.thinking_level = thinking_level;
                self.snapshot.revision += 1;
                Ok(self.snapshot())
            }
            Command::Abort { .. } => {
                self.snapshot.phase = SessionPhase::Idle;
                self.snapshot.revision += 1;
                Ok(self.snapshot())
            }
            Command::Prompt { .. } | Command::Steer { .. } => Err(ProtocolError {
                code: ProtocolErrorCode::NotImplemented,
                message: "Rust Agent Loop 尚未接管 prompt 或 steer；请使用 TypeScript fallback。"
                    .to_owned(),
                details: None,
            }),
            _ => Err(ProtocolError {
                code: ProtocolErrorCode::InvalidRequest,
                message: "该命令需要由上层 Session Manager 处理。".to_owned(),
                details: None,
            }),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::SessionRuntime;
    use protocol::{Command, ModelRef, ProtocolErrorCode, SessionPhase, ThinkingLevel};

    fn runtime() -> SessionRuntime {
        SessionRuntime::new(
            "session-1",
            ".",
            ModelRef {
                provider: "test".to_owned(),
                id: "model".to_owned(),
            },
        )
    }

    #[test]
    fn control_commands_update_snapshot_without_exposing_mutable_state() {
        let mut runtime = runtime();
        let updated = runtime
            .execute(Command::SetThinking {
                session_id: "session-1".to_owned(),
                thinking_level: ThinkingLevel::High,
            })
            .expect("control command should succeed");

        assert_eq!(updated.thinking_level, ThinkingLevel::High);
        assert_eq!(updated.revision, 1);
        assert_eq!(runtime.snapshot().phase, SessionPhase::Idle);
    }

    #[test]
    fn unsupported_prompt_is_explicitly_rejected() {
        let mut runtime = runtime();
        let error = runtime
            .execute(Command::Prompt {
                session_id: "session-1".to_owned(),
                text: "hello".to_owned(),
            })
            .expect_err("prompt must remain on the fallback path");

        assert_eq!(error.code, ProtocolErrorCode::NotImplemented);
    }
}

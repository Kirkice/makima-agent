# Pi Agent Rust 重构骨架

本目录是 Rust/TypeScript 混合重构的第一阶段实现。

## 当前范围

- `pi-protocol`：跨语言协议模型。
- `pi-core`：最小 Session Runtime，暂时只处理控制命令。
- `pi-cli`：可以独立启动的 Rust CLI 骨架。

当前 Rust 代码**不会接管完整 Agent 流程**，也不会调用 Provider、执行工具或修改现有 Session 数据。`prompt`、Agent Loop、Session Store、Tool Runtime、Sandbox 和 RPC Server 会在协议与行为测试完成后逐步迁移。

## 本地验证

```text
cargo test --manifest-path pi-runtime/Cargo.toml
cargo run --manifest-path pi-runtime/Cargo.toml --bin pi-rust
```

## 设计原则

- 只通过 `pi-protocol` 交换数据，不共享内部对象。
- Core 只负责状态和业务，不依赖 TUI 或 Provider SDK。
- 未迁移的功能必须明确返回错误并继续使用 TypeScript fallback。
- 每个迁移阶段都需要行为回放、协议校验和回归测试。

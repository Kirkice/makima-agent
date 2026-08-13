# Rust 模块目录

本目录下每个一级子目录都是一个独立 Cargo crate，类似 C++ 解决方案中的独立项目。crate 只通过其公开 API 和 [`../Cargo.toml`](../Cargo.toml) 中的 workspace 配置协作；Cargo 根据各自 [`Cargo.toml`](../Cargo.toml) 中的 `path` 依赖自动安排编译顺序。

## 已实现模块

| Crate | 职责 | 可依赖的 Pi crate | 当前状态 |
| --- | --- | --- | --- |
| [`protocol`](protocol/Cargo.toml) | Rust/TypeScript 共享协议模型 | 无 | 已实现 |
| [`session`](session/Cargo.toml) | JSONL v4 Session Store、状态归约和恢复 | 无 | 已实现，未接管生产路径 |
| [`sandbox`](sandbox/Cargo.toml) | 权限策略、TS 兼容配置、`srt` 隔离后端适配与命令执行 | 无 | 已实现，Windows 禁用；尚未接管工具执行 |
| [`runtime`](runtime/Cargo.toml) | 过渡期最小 Runtime 骨架 | `protocol`、`session` | 临时实现 |
| [`cli`](cli/Cargo.toml) | Rust CLI 进程入口 | `runtime`、`protocol` | 第一阶段骨架 |

## 后续模块预留

新增模块必须创建独立 crate，不把功能继续堆入 [`runtime`](runtime/Cargo.toml)。

```text
crates/
├── protocol/          # 共享协议，始终位于依赖图底部
├── session/           # 已完成
├── sandbox/           # 已完成 TS 兼容配置、生命周期和 srt 隔离执行器
├── tool-runtime/      # 依赖 sandbox
├── agent-loop/        # 依赖 Tool Runtime Port
├── agent-session/     # 依赖 agent-loop 与 session
├── rpc/               # 依赖 agent-session
└── cli/               # 最外层入口，依赖 rpc
```

## 测试位置

- crate 内部的细粒度单元测试放在 `src/` 文件旁的 `#[cfg(test)]` 模块。
- 可独立执行、后续可整体删除的协议/跨语言测试放在 `<crate>/tests/`。
- 固定输入样例放在 `<crate>/tests/fixtures/`。

例如 Session Store 的临时跨语言兼容测试和 fixture 分别位于 [`session/tests/jsonl_v4_conformance.rs`](session/tests/jsonl_v4_conformance.rs) 与 [`session/tests/fixtures`](session/tests/fixtures)。

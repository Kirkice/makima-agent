# Pi Agent Rust Runtime

`makima-runtime` 是 Makima Agent 的 Rust 运行时工作区。它采用一个 Cargo workspace 管理多个独立 crate，等价于 C++ 工程中“一个解决方案包含多个项目”的组织方式。

每个 crate 都有自己的 [`Cargo.toml`](Cargo.toml)、源码和测试，可以独立执行 `cargo test -p <crate>`；根 Cargo workspace 负责统一版本、共享依赖和全量构建。Cargo 会从 crate manifest 的依赖声明自动计算构建顺序，不需要维护手工顺序脚本。

## 当前目录

```text
makima-runtime/
├── Cargo.toml                 # workspace、共享依赖与默认构建成员
├── crates/
│   ├── protocol/              # 跨语言 Command / Event / Snapshot 契约
│   ├── session/               # JSONL v4 Session Store
│   ├── sandbox/               # TS 兼容的策略、配置与 OS 隔离后端适配
│   ├── runtime/               # AgentSession、RPC listener 与 Provider Host IPC 编排层
│   └── cli/                   # Rust 进程入口
└── README.md
```

各 crate 的职责、依赖边界和后续规划见 [`crates/README.md`](crates/README.md)。

## 当前依赖图

```text
protocol ───────┐
agent-loop ─────┼── runtime ─── cli
session ────────┘
```

- `protocol`：最底层共享数据契约，不依赖其他 Rust 模块。
- `session`：独立持久化模块，不依赖 Agent Loop、Provider、TUI 或工具运行时。
- `sandbox`：提供权限决策、与 TypeScript Extension 兼容的配置合并、`srt` 命令包装与执行生命周期；Windows 明确禁用，Linux/macOS 通过 `srt` 接入现有 OS 隔离后端。
- `runtime`：包含生产 JSONL Session 工厂、`AgentSession` 命令状态机、stdio/Unix socket RPC listener，以及 Rust Core 到 TypeScript Provider Host 的 framed-CBOR 传输。
- `cli`：启动 stdio framed-CBOR RPC 服务；stdout 不写诊断文本，Session 根目录和默认模型由环境变量配置。

## 规划依赖图

后续模块按依赖方向新增 crate，而不是扩大现有 crate：

```text
protocol
├── session
├── sandbox
├── tool-runtime ───────> sandbox
├── agent-loop ─────────> tool-runtime
├── agent-session ──────> agent-loop, session
├── rpc ────────────────> agent-session
└── cli ────────────────> rpc
```

Provider、扩展与 TUI 保持在 TypeScript Host，通过版本化协议与 Rust 交互。模块间只能依赖公开 crate API；禁止通过文件路径、内部结构体或 TypeScript 内部对象跨层访问。

## 常用构建命令

```text
# 构建当前 workspace；Cargo 自动先构建依赖 crate。
cargo check --manifest-path makima-runtime/Cargo.toml

# 验证全部默认成员。
cargo test --manifest-path makima-runtime/Cargo.toml

# 仅验证 Session Store，适合模块级开发。
cargo test --manifest-path makima-runtime/Cargo.toml -p session

# 对所有 crate 执行静态检查。
cargo clippy --manifest-path makima-runtime/Cargo.toml --workspace --all-targets --all-features -- -D warnings
```

## 迁移状态

- `session`：JSONL v4 Store、状态归约、查询、统计、repository 单写者租约与 fixture conformance 测试已完成。生产 `AgentSessionFactory` 在 Session 生命周期内保留该租约。
- `sandbox`：已实现策略层、全局/项目 `sandbox.json` 加载与 TypeScript 一致的覆盖顺序、`--no-sandbox`/配置/平台生命周期状态、`srt` 命令包装及同步执行器。Windows 与现有 Extension 一致地禁用；Linux/macOS 要求应用分发或在 `PATH` 中提供 `srt`，由其调用 `bubblewrap` 或 `sandbox-exec` 实施 OS 级隔离。尚未接入 Tool Runtime 生产路径。
- `runtime`：stdio/Unix socket listener 会在连接结束时回收 `ConnectionSessionHandler` 订阅；生产工厂使用 `AgentSession` 接收 `prompt`、`steer` 和 `abort`，并持久化稳定 transcript/configuration。Provider Host 的 request/stream/abort DTO、framed-CBOR client 与子进程生命周期已实现，但尚未接入 `AgentManagedSession` 的 Agent Loop 驱动。
- `agent-loop`、`tool-runtime`、`agent-session`、`rpc`：已创建并具有模块级测试；Tool Runtime continuation、thinking delta 投影与 Provider stream 到 `SessionProgress` 的生产接线仍待完成。

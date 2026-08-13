# Pi Agent Rust Runtime

`pi-runtime` 是 Pi Agent 的 Rust 运行时工作区。它采用一个 Cargo workspace 管理多个独立 crate，等价于 C++ 工程中“一个解决方案包含多个项目”的组织方式。

每个 crate 都有自己的 [`Cargo.toml`](Cargo.toml)、源码和测试，可以独立执行 `cargo test -p <crate>`；根 Cargo workspace 负责统一版本、共享依赖和全量构建。Cargo 会从 crate manifest 的依赖声明自动计算构建顺序，不需要维护手工顺序脚本。

## 当前目录

```text
pi-runtime/
├── Cargo.toml                 # workspace、共享依赖与默认构建成员
├── crates/
│   ├── protocol/              # 跨语言 Command / Event / Snapshot 契约
│   ├── session/               # JSONL v4 Session Store
│   ├── runtime/               # 临时 Runtime 骨架，后续收敛为 Agent Session 编排层
│   └── cli/                   # Rust 进程入口
└── README.md
```

各 crate 的职责、依赖边界和后续规划见 [`crates/README.md`](crates/README.md)。

## 当前依赖图

```text
protocol ───────┐
                ├── runtime ─── cli
session ────────┘       │
                         └── session（CLI 临时验证用途）
```

- `protocol`：最底层共享数据契约，不依赖其他 Rust 模块。
- `session`：独立持久化模块，不依赖 Agent Loop、Provider、TUI 或工具运行时。
- `runtime`：当前仅包含最小 Runtime；后续拆分完成后由 `agent-session` 取代其 Session 编排职责。
- `cli`：仅负责进程启动与命令行边界，不实现业务状态机。

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
cargo check --manifest-path pi-runtime/Cargo.toml

# 验证全部默认成员。
cargo test --manifest-path pi-runtime/Cargo.toml

# 仅验证 Session Store，适合模块级开发。
cargo test --manifest-path pi-runtime/Cargo.toml -p session

# 对所有 crate 执行静态检查。
cargo clippy --manifest-path pi-runtime/Cargo.toml --workspace --all-targets --all-features -- -D warnings
```

## 迁移状态

- `session`：JSONL v4 Store、状态归约、查询、统计、typed provisioning 与独立 fixture conformance 测试已完成；尚未替换 TypeScript 生产路径。
- `runtime`：仅作为过渡期最小 Runtime；`prompt` 与 `steer` 继续回退 TypeScript。
- `agent-loop`、`tool-runtime`、`sandbox`、`agent-session`、`rpc`：尚未创建，必须按上图逐一独立实现和验收。

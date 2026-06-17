# Makima Agent

Makima Agent 是一个面向"成熟 Agent 能力 + 拟人化表现层"方向的全栈 Agent 后端工程。

## 核心架构

- **LangGraph** — Agent 编排骨架（多步任务编排、中断与恢复）
- **OpenHands** — 任务执行与工具运行参考
- **Mem0** — 长期记忆层
- **Rust Tool Runtime** — 高性能工具执行服务（gRPC）
- 后续接入 Unity / 其他前端形态

采用 **单体后端 + 高性能工具微服务** 架构：核心 Agent 能力集中在 `apps/backend/` 中实现，工具执行层通过独立的 Rust gRPC 服务提供高性能和安全隔离。

## 技术栈

| 层 | 技术 |
|---|---|
| 后端 API | Python 3.10+ / FastAPI / SQLAlchemy 2.0 |
| Agent 编排 | LangGraph / LangChain |
| 工具执行 | **Rust** / tokio / tonic (gRPC) |
| 记忆层 | Mem0 / pgvector |
| 知识库 | LlamaIndex / RAG |
| 任务队列 | Celery / Redis |
| 可观测性 | OpenTelemetry / Prometheus |
| 数据库 | PostgreSQL (asyncpg) |
| 部署 | Docker Compose |

## 目录结构

```text
makima-agent/
├── apps/
│   └── backend/                  # FastAPI 主服务（单体后端）
│       └── src/makima/
│           ├── app.py            # 应用入口
│           ├── core/             # 基础设施（db、deps、middleware）
│           ├── auth/             # 认证（User 模型、JWT）
│           ├── sessions/         # 会话（Session、Message 模型）
│           ├── tasks/            # 任务（Task 模型）
│           ├── routes/           # API 路由
│           ├── audit/            # 审计日志
│           ├── security/         # RBAC 权限
│           ├── config_center/    # 配置中心（Redis）
│           ├── knowledge/        # 知识库 RAG
│           ├── memory/           # 记忆服务（Mem0）
│           ├── orchestrator/     # LangGraph 编排器
│           ├── queue/            # Celery 任务队列
│           ├── observability/    # 可观测性
│           ├── tools/            # 工具（Python 实现 + Rust 客户端）
│           └── clients/          # 外部客户端封装
├── services/
│   └── tool-runtime/             # 🔧 Rust 高性能工具服务（gRPC）
│       ├── proto/                # gRPC 接口定义
│       └── src/
│           ├── sandbox/          # 安全沙箱（路径/命令/网络）
│           ├── tools/            # 工具实现（shell/file/http）
│           ├── document/         # 文档分块处理
│           └── server.rs         # gRPC 服务实现
├── packages/
│   ├── common/                   # 公共工具包（配置、日志）
│   └── schemas/                  # 事件协议、DTO
├── external/                     # 外部依赖源码参考
│   ├── langgraph/
│   ├── openhands/
│   └── mem0/
├── docs/                         # 文档
│   └── architecture/             # 架构文档
├── infra/                        # 基础设施
│   └── docker/                   # Dockerfile
├── docker-compose.yml
├── Makefile
└── .env.example
```

## 快速开始

### 1. 环境准备

- Python >= 3.10
- Rust (stable, 用于 Tool Runtime)
- Docker & Docker Compose
- protoc 编译器

### 2. 安装依赖

```bash
# Python 包
pip install -e packages/common
pip install -e packages/schemas
pip install -e apps/backend

# Rust Tool Runtime
cd services/tool-runtime
cargo build --release
```

### 3. 配置

复制 `.env.example` 为 `apps/backend/.env` 并编辑：

```env
# LLM 配置（支持 OpenAI / DeepSeek / 其他兼容接口）
LLM_API_KEY=your-api-key
LLM_API_BASE=https://api.deepseek.com
LLM_MODEL=deepseek-chat

# 数据库
DATABASE_URL=postgresql+asyncpg://makima:makima@localhost:5432/makima

# Redis
REDIS_URL=redis://localhost:6379/0
```

### 4. 启动服务

```bash
# Docker 一键启动全部服务
docker compose up -d

# 或者手动分别启动：

# 启动基础设施（PostgreSQL + Redis）
docker compose up -d postgres redis

# 启动 Python 后端
cd apps/backend
python -m makima.app

# 启动 Rust Tool Runtime（新终端）
cd services/tool-runtime
cargo run --release
```

### 5. 访问

- **API 文档**: http://localhost:8000/docs
- **健康检查**: http://localhost:8000/health
- **gRPC 服务**: localhost:50051

## Rust Tool Runtime

高性能工具执行层，通过 gRPC 与 Python 后端通信：

| 服务 | 功能 |
|------|------|
| `ShellService` | Shell 命令执行（带超时和命令过滤） |
| `FileService` | 文件读写（路径遍历防护） |
| `HttpService` | HTTP 请求（内网 URL 过滤） |
| `DocumentService` | 文本分块 + Token 估算 |
| `SandboxService` | 路径/命令/URL 安全检查 |

**性能对比**: Shell 执行 10x, 文件 I/O 20x, 文本分块 50x（vs Python）

Python 端通过 `makima.tools.rust_client.RustToolClient` 调用 Rust 服务，Rust 不可用时自动降级到 Python 实现。

## 架构说明

本项目采用 **单体后端 + 高性能工具微服务** 架构：

- `apps/backend/` 是核心单体服务，集中实现了 API、编排、记忆、知识库、认证、审计、任务队列等全部功能
- `services/tool-runtime/` 是独立的 Rust gRPC 微服务，提供高性能和安全沙箱的工具执行能力
- 外部依赖参考代码在 `external/` 下，通过 pip install 依赖而非直接耦合

这种架构在当前阶段兼顾了开发效率和执行性能，未来如有需要可逐步将单体拆分为微服务。

## 项目阶段

| 阶段 | 内容 | 状态 |
|------|------|------|
| Phase 0 | 工程骨架 | ✅ |
| Phase 1 | 最小 Agent 后端 | ✅ |
| Phase 2 | 工具执行 | ✅ |
| Phase 3 | 记忆与知识库 | ✅ |
| Phase 4 | 产品化能力 | ✅ |
| Phase 5 | 客户端接入 | 🔲 |

详细路线见：[Agent 后端路线文档](docs/architecture/agent-backend-roadmap.md)

## License

MIT
# Makima Agent

Makima Agent 是一个面向"成熟 Agent 能力 + 拟人化表现层"方向的全栈 Agent 后端工程。

## 核心架构

- **LangGraph** — Agent 编排骨架（多步任务编排、中断与恢复）
- **OpenHands** — 任务执行与工具运行参考
- **Mem0** — 长期记忆层
- **Rust Tool Runtime** — 高性能工具执行服务（gRPC）
- **LiveKit Agents** — 实时语音通话（VAD + STT + LLM + TTS）
- 后续接入 Unity / 其他前端形态

采用 **单体后端 + 高性能工具微服务 + 语音运行时** 架构：核心 Agent 能力集中在 `apps/backend/` 中实现，工具执行层通过独立的 Rust gRPC 服务提供高性能和安全隔离，语音通话通过 LiveKit Cloud 实现实时对话。

## 功能特性

### 核心功能

- **多模式系统** — 6种预设模式（Code/Architect/Ask/Debug/Chat/Companion），每种模式可配置不同的模型、温度和工具权限
- **PromptEngine** — 模块化的 Prompt 工程引擎，支持动态组装系统提示词
- **人设系统** — 可配置的玛奇玛人设（冷静、优雅、克制），通过 YAML 配置
- **模式配置外置** — 所有模式配置存储在 `.makima/modes.yaml`，支持热更新

### 高级特性（借鉴 Zoo Code）

- **API 重试机制** — 指数退避重试（5s→10s→20s→40s→80s），支持 429 限流
- **工具审批系统** — 基于风险等级的审批（low/medium/high），高风险工具需要用户确认
- **上下文管理** — 自动压缩历史消息，防止 token 超限
- **文件追踪** — 追踪文件变更，防止冲突
- **检查点回滚** — 支持任务检查点和回滚
- **流式响应** — SSE 流式输出，实时显示 Agent 思考和执行过程

### 语音通话功能

- **实时语音对话** — 基于 LiveKit Agents SDK 的 WebRTC 语音通话
- **VAD 语音活动检测** — Silero VAD 模型，准确检测语音
- **STT 语音识别** — 支持 Deepgram（高质量）和 OpenAI Whisper（免费）
- **TTS 语音合成** — OpenAI TTS，多种音色可选
- **语音打断** — 支持随时打断 Agent 说话
- **CLI 语音客户端** — 命令行语音客户端，无需前端即可通话

### 工具系统

- **文件操作** — 读/写/列表，带沙箱保护
- **Shell 命令** — 命令执行，带超时和危险命令过滤
- **HTTP 请求** — HTTP 客户端，带内网地址过滤
- **Rust 高性能工具** — Shell/File/HTTP/Document/Sandbox 的 Rust 实现（10-50x 性能提升）

### 记忆与知识库

- **长期记忆** — 基于 Mem0 的长期记忆系统
- **知识库 RAG** — 基于 LlamaIndex 的文档检索增强生成
- **记忆注入** — 自动将相关记忆注入到对话上下文

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
| 数据库 | PostgreSQL (asyncpg) / SQLite (开发) |
| 语音通话 | LiveKit Agents / Silero VAD / Deepgram STT / OpenAI TTS |
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
│           ├── clients/          # 外部客户端封装（LLM、Rust、retry）
│           ├── modes/            # 多模式系统
│           ├── persona/          # 人设系统
│           └── prompts/          # PromptEngine
├── services/
│   ├── tool-runtime/             # 🔧 Rust 高性能工具服务（gRPC）
│   │   ├── proto/                # gRPC 接口定义
│   │   └── src/
│   │       ├── sandbox/          # 安全沙箱（路径/命令/网络）
│   │       ├── tools/            # 工具实现（shell/file/http）
│   │       ├── document/         # 文档分块处理
│   │       └── server.rs         # gRPC 服务实现
│   └── voice-runtime/            # 🎙️ 语音通话服务（LiveKit）
│       ├── agent.py              # Voice Agent Worker
│       ├── client.py             # CLI 语音客户端
│       ├── test_voice.py         # 测试脚本
│       └── pyproject.toml        # 依赖声明
├── packages/
│   ├── common/                   # 公共工具包（配置、日志）
│   └── schemas/                  # 事件协议、DTO
├── .makima/                      # 配置文件目录
│   ├── modes.yaml                # 模式配置（6种模式）
│   └── persona.yaml              # 人设配置（玛奇玛）
├── external/                     # 外部依赖源码参考
│   ├── langgraph/
│   ├── openhands/
│   └── mem0/
├── docs/                         # 文档
│   └── architecture/             # 架构文档
├── infra/                        # 基础设施
│   └── docker/                   # Dockerfile
├── cli.py                        # CLI 客户端
├── launcher.py                   # 一键启动器
├── docker-compose.yml
├── Makefile
└── .env.example
```

## 环境依赖

### 必需依赖

- **Python** >= 3.10
- **Rust** stable（用于 Tool Runtime）
- **protoc** 编译器（用于 gRPC）

### 可选依赖

- **Docker & Docker Compose**（用于容器化部署）
- **Redis**（用于任务队列和配置中心）
- **PostgreSQL**（生产环境数据库，开发可用 SQLite）
- **LiveKit Cloud 账号**（用于语音通话，免费额度）

### Python 依赖

#### 后端核心依赖（apps/backend/pyproject.toml）

```bash
# 核心框架
fastapi>=0.104.0
uvicorn[standard]>=0.24.0
sqlalchemy[asyncio]>=2.0.23
alembic>=1.12.1

# Agent 编排
langgraph>=0.0.40
langchain>=0.1.0
langchain-openai>=0.0.5

# 记忆与知识库
mem0ai>=0.0.1
llama-index>=0.10.0
pgvector>=0.2.4

# 任务队列
celery>=5.3.4
redis>=5.0.1

# 可观测性
opentelemetry-api>=1.21.0
opentelemetry-sdk>=1.21.0
opentelemetry-exporter-otlp>=1.21.0
prometheus-client>=0.19.0

# 工具
httpx>=0.25.2
pydantic>=2.5.2
pydantic-settings>=2.1.0
python-jose[cryptography]>=3.3.0
passlib[bcrypt]>=1.7.4
python-multipart>=0.0.6
```

#### 语音通话依赖（services/voice-runtime/pyproject.toml）

```bash
livekit-agents>=0.8.0
livekit-plugins-openai>=0.5.0
livekit-plugins-silero>=0.3.0
livekit-plugins-deepgram>=0.3.0  # 可选，高质量 STT
sounddevice>=0.4.6
numpy>=1.24.0
python-dotenv>=1.0.0
```

#### CLI 客户端依赖

```bash
rich>=13.7.0
prompt-toolkit>=3.0.43
httpx>=0.25.2
```

### Rust 依赖（services/tool-runtime/Cargo.toml）

```toml
tokio = { version = "1", features = ["full"] }
tonic = "0.11"
prost = "0.12"
sha2 = "0.10"
chrono = "0.4"
tiktoken-rs = "0.5"
serde = { version = "1", features = ["derive"] }
serde_json = "1"
```

## 快速开始

### 1. 克隆项目

```bash
git clone https://github.com/Kirkice/makima-agent.git
cd makima-agent
git submodule update --init --recursive
```

### 2. 安装依赖

```bash
# Python 公共包
pip install -e packages/common
pip install -e packages/schemas

# Python 后端
pip install -e apps/backend

# Python CLI 客户端
pip install rich prompt-toolkit httpx

# Rust Tool Runtime（可选，不安装会自动降级到 Python 实现）
cd services/tool-runtime
cargo build --release
cd ../..

# 语音通话依赖（可选）
pip install livekit-agents livekit-plugins-openai livekit-plugins-silero sounddevice numpy python-dotenv
```

### 3. 配置环境变量

编辑 `apps/backend/.env`：

```env
# ── LLM 配置 ──────────────────────────────────
MAKIMA_LLM_API_KEY=sk-your-api-key
MAKIMA_LLM_API_BASE=https://api.deepseek.com  # 或其他 OpenAI 兼容 API
MAKIMA_LLM_MODEL=deepseek-chat
MAKIMA_LLM_TEMPERATURE=0.7
MAKIMA_LLM_MAX_TOKENS=4096

# ── 数据库 ──────────────────────────────────────
MAKIMA_DATABASE_URL=sqlite+aiosqlite:///./makima.db  # 开发环境
# MAKIMA_DATABASE_URL=postgresql+asyncpg://user:pass@localhost:5432/makima  # 生产环境

# ── Redis（可选）──────────────────────────────────
MAKIMA_REDIS_URL=redis://localhost:6379/0

# ── LiveKit 语音通话（可选）────────────────────────
LIVEKIT_URL=wss://your-project.livekit.cloud
LIVEKIT_API_KEY=your-api-key
LIVEKIT_API_SECRET=your-api-secret

# ── 其他配置 ──────────────────────────────────────
MAKIMA_DEBUG=false
MAKIMA_API_SECRET_KEY=replace-with-a-long-random-secret
MAKIMA_CLI_USERNAME=your-local-admin
MAKIMA_CLI_PASSWORD=replace-with-a-strong-local-password
```

### 4. 启动后端服务

```bash
cd apps/backend
python -m makima.app
```

服务启动后访问：
- **API 文档**: http://localhost:8000/docs
- **健康检查**: http://localhost:8000/health

### 5. 启动 CLI 客户端

```bash
# 方式一：使用 launcher.py（自动启动后端 + CLI）
python launcher.py

# 方式二：单独启动 CLI（需要先启动后端）
python cli.py
```

### 6. 启动语音通话（可选）

需要配置 LiveKit Cloud 凭证。

```bash
# 终端 1：启动 Voice Agent Worker
cd services/voice-runtime
python agent.py dev

# 终端 2：启动 CLI 语音客户端
cd services/voice-runtime
python client.py --room makima-voice-room
```

然后对着麦克风说话，玛奇玛会用语音回复你！

## 使用指南

### 文本对话

```bash
python cli.py
```

在 CLI 中输入文字与玛奇玛对话，支持：
- 多轮对话
- 工具调用（文件操作、Shell 命令等）
- 记忆检索
- 知识库查询

### 切换模式

```bash
# 在 CLI 中使用 /mode 命令
/mode code      # 切换到 Code 模式（编程助手）
/mode architect # 切换到 Architect 模式（架构设计）
/mode ask       # 切换到 Ask 模式（问答）
/mode debug     # 切换到 Debug 模式（调试）
/mode chat      # 切换到 Chat 模式（闲聊）
/mode companion # 切换到 Companion 模式（陪伴）
```

### 自定义模式

编辑 `.makima/modes.yaml`，可以：
- 修改现有模式的配置
- 添加新模式
- 为不同模式配置不同的 LLM 模型

```yaml
modes:
  - slug: code
    name: 🛠️ Code
    model: deepseek-chat  # 使用 DeepSeek
    temperature: 0.0
    tool_groups: [read, write, command, network]
    
  - slug: architect
    name: 🏗️ Architect
    model: gpt-4o  # 使用 GPT-4o
    api_base: https://api.openai.com/v1
    api_key: sk-xxx
    temperature: 0.1
    tool_groups: [read]
```

### 自定义人设

编辑 `.makima/persona.yaml`，可以修改玛奇玛的人设：

```yaml
name: Makima
identity: |
  玛奇玛，来自《电锯人》的角色。
  冷静、优雅、有控制力。

personality: |
  - 说话简洁有力
  - 表面温和，实际边界感很强
  - 先给判断，再给理由

catchphrases:
  - "我知道了。"
  - "先别急。"
  - "继续说。"
```

### 语音通话

1. 确保已配置 LiveKit Cloud 凭证
2. 启动 Voice Agent Worker：
   ```bash
   cd services/voice-runtime
   python agent.py dev
   ```
3. 启动 CLI 语音客户端（新终端）：
   ```bash
   cd services/voice-runtime
   python client.py --room makima-voice-room
   ```
4. 对着麦克风说话，玛奇玛会用语音回复

## API 端点

### 认证

```bash
# 登录
POST /api/auth/login
{
  "username": "your-local-admin",
  "password": "replace-with-a-strong-local-password"
}

# 返回 JWT token
{
  "access_token": "eyJ...",
  "token_type": "bearer"
}
```

### 会话

```bash
# 创建会话
POST /api/sessions
Authorization: Bearer <token>

# 发送消息（SSE 流式响应）
POST /api/sessions/{session_id}/messages
{
  "content": "你好，玛奇玛"
}

# 列出会话
GET /api/sessions
```

### 模式

```bash
# 获取所有模式
GET /api/modes

# 切换模式
POST /api/sessions/{session_id}/mode
{
  "mode_slug": "code"
}
```

### 人设

```bash
# 获取当前人设
GET /api/persona

# 更新人设
PUT /api/persona
{
  "name": "Makima",
  "personality": "..."
}

# 重载人设配置
POST /api/persona/reload
```

### 检查点

```bash
# 创建检查点
POST /api/checkpoints
{
  "session_id": "xxx",
  "label": "before-refactor"
}

# 列出检查点
GET /api/checkpoints?session_id=xxx

# 回滚到检查点
POST /api/checkpoints/{checkpoint_id}/restore
```

### 审批

```bash
# 响应审批请求
POST /api/approvals/{request_id}/respond
{
  "approved": true
}
```

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

本项目采用 **单体后端 + 高性能工具微服务 + 语音运行时** 架构：

- `apps/backend/` 是核心单体服务，集中实现了 API、编排、记忆、知识库、认证、审计、任务队列等全部功能
- `services/tool-runtime/` 是独立的 Rust gRPC 微服务，提供高性能和安全沙箱的工具执行能力
- `services/voice-runtime/` 是语音通话服务，基于 LiveKit Agents SDK
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
| Phase 5 | 多模式系统 | ✅ |
| Phase 6 | PromptEngine | ✅ |
| Phase 7 | 人设系统 | ✅ |
| Phase 8 | 高级特性（重试/审批/上下文/检查点） | ✅ |
| Phase 9 | 语音通话 | ✅ |
| Phase 10 | 前端客户端 | 🔲 |

详细路线见：[Agent 后端路线文档](docs/architecture/agent-backend-roadmap.md)

## 常见问题

### Q: Rust Tool Runtime 必须安装吗？

A: 不是必须的。如果不安装，系统会自动降级到 Python 实现，功能完全相同，只是性能稍低。

### Q: 如何更换 LLM 模型？

A: 修改 `apps/backend/.env` 中的 `MAKIMA_LLM_MODEL` 和 `MAKIMA_LLM_API_BASE`。支持任何 OpenAI 兼容的 API（DeepSeek、OpenAI、本地 Ollama 等）。

### Q: 语音通话必须用 LiveKit Cloud 吗？

A: 可以自部署 LiveKit Server，但需要 Docker。LiveKit Cloud 提供免费额度，开发测试足够用。

### Q: 如何添加新的工具？

A: 在 `apps/backend/src/makima/tools/` 目录下创建新的工具文件，然后在 `tools/__init__.py` 中注册。参考现有工具的实现。

### Q: 如何让玛奇玛记住我？

A: 系统会自动保存对话记忆。确保 `apps/backend/.env` 中启用了记忆功能（`MAKIMA_MEMORY_ENABLED=true`）。

## License

MIT

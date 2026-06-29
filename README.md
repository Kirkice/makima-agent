# Makima Agent

一个基于 Rust + egui 构建的 AI Agent 桌面客户端，采用"成熟 Agent 能力 + 拟人化表现层"架构。

## 核心架构

- **LangGraph** — Agent 编排骨架（多步任务编排、中断与恢复）
- **OpenHands** — 任务执行与工具运行参考
- **Mem0** — 长期记忆层
- **Rust Tool Runtime** — 高性能工具执行服务（gRPC）
- **LiveKit RTC + Fish Audio** — 实时语音通话
- **MCP Marketplace** — MCP 服务器市场，支持一键安装/卸载

采用 **单体后端 + 高性能工具微服务 + 语音运行时** 架构。

## 功能特性

### 核心功能

- **多模式系统** — 6种预设模式（Code/Architect/Ask/Debug/Chat/Companion）
- **PromptEngine** — 模块化的 Prompt 工程引擎
- **人设系统** — 可配置的玛奇玛人设（冷静、优雅、克制）
- **模式配置外置** — 所有模式配置存储在 `.makima/modes.yaml`

### 高级特性（借鉴 Zoo Code）

- **API 重试机制** — 指数退避重试（5s→10s→20s→40s→80s）
- **工具审批系统** — 基于风险等级的审批（low/medium/high）
- **上下文管理** — 自动压缩历史消息，防止 token 超限
- **文件追踪** — 追踪文件变更，防止冲突
- **检查点回滚** — 支持任务检查点和回滚
- **流式响应** — SSE 流式输出

### MCP Marketplace

内置 MCP（Model Context Protocol）服务器市场，提供 15 个常用 MCP 服务器的一键安装：

- **brave-search** — Brave 搜索引擎
- **filesystem** — 文件系统操作
- **context7** — 代码文档和示例
- **github** — GitHub 集成
- **postgres** — PostgreSQL 数据库
- **sqlite** — SQLite 数据库
- **fetch** — 网络请求
- **sequential-thinking** — 顺序思维链
- **memory** — 持久化记忆
- **puppeteer** — 浏览器自动化
- **exa** — Exa 搜索
- **e2b** — E2B 沙箱执行
- **figma** — Figma 设计工具
- **redis** — Redis 缓存
- **time** — 时间工具

特性：
- 支持多种安装方式（NPX、Docker、本地二进制等）
- 参数模板替换（API Key 等敏感信息）
- 标签过滤和搜索
- 安装状态跟踪
- 一键卸载

### 语音通话功能

- **实时语音对话** — 基于 LiveKit RTC (WebRTC)
- **静音检测** — RMS 阈值检测
- **STT 语音识别** — Fish Audio ASR，支持中英文
- **TTS 语音合成** — Fish Audio TTS，支持声音克隆
- **后端集成** — 复用完整 Agent 能力
- **CLI 语音客户端** — 命令行语音客户端

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
| 前端客户端 | **Rust** / egui / eframe |
| 后端 API | Python 3.10+ / FastAPI / SQLAlchemy 2.0 |
| Agent 编排 | LangGraph / LangChain |
| 工具执行 | **Rust** / tokio / tonic (gRPC) |
| 记忆层 | Mem0 / pgvector |
| 知识库 | LlamaIndex / RAG |
| 任务队列 | Celery / Redis |
| 可观测性 | OpenTelemetry / Prometheus |
| 数据库 | PostgreSQL (asyncpg) / SQLite (开发) |
| 语音通话 | LiveKit RTC / Fish Audio ASR / Fish Audio TTS |
| 部署 | Docker Compose |

## 目录结构

```text
makima-agent/
├── apps/
│   ├── desktop-egui/             # Rust/egui 桌面客户端
│   │   ├── src/
│   │   │   ├── main.rs           # 应用入口
│   │   │   ├── app.rs            # 应用状态和命令处理
│   │   │   ├── api/              # API 客户端（auth, sessions, tasks, marketplace 等）
│   │   │   ├── state/            # 状态管理（app, chat, settings, task, voice）
│   │   │   ├── ui/               # UI 组件（shell, panels, chat）
│   │   │   └── theme.rs          # 主题系统
│   │   └── Cargo.toml            # Rust 依赖
│   └── backend/                  # FastAPI 主服务（单体后端）
│       ├── assets/marketplace/   # MCP 市场数据
│       │   └── mcps.yml          # MCP 服务器配置（15个）
│       └── src/makima/
│           ├── app.py            # 应用入口
│           ├── core/             # 基础设施（db、deps、middleware）
│           ├── auth/             # 认证（User 模型、JWT）
│           ├── sessions/         # 会话（Session、Message 模型）
│           ├── tasks/            # 任务（Task 模型）
│           ├── routes/           # API 路由
│           │   ├── marketplace.py # MCP 市场 API
│           │   ├── mcp.py        # MCP 服务器管理
│           │   └── ...
│           ├── marketplace/      # MCP 市场服务
│           │   ├── models.py     # 数据模型
│           │   ├── loader.py     # 配置加载器
│           │   ├── installer.py  # 安装/卸载服务
│           │   └── service.py    # 业务逻辑层
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
│   └── voice-runtime/            # 🎙️ 语音通话服务（LiveKit RTC + Fish Audio）
│       ├── agent.py              # 简单语音循环
│       ├── fish_audio.py         # Fish Audio ASR/TTS 异步封装
│       └── pyproject.toml        # 依赖声明
├── packages/
│   ├── common/                   # 公共工具包（配置、日志）
│   └── schemas/                  # 事件协议、DTO
├── .makima/                      # 配置文件目录
│   ├── modes.yaml                # 模式配置（6种模式）
│   ├── persona.yaml              # 人设配置（玛奇玛）
│   └── mcp.json                  # 已安装的 MCP 服务器（运行时生成）
├── external/                     # 外部依赖源码参考
├── docs/                         # 文档
├── infra/                        # 基础设施
├── cli.py                        # CLI 客户端
├── launcher.py                   # 一键启动器
├── makima-server                 # 服务器启动脚本（Unix）
├── makima-server.bat             # 服务器启动脚本（Windows）
├── docker-compose.yml
├── Makefile
└── .env.example
```

## 环境依赖

### 必需依赖

- **Rust** >= 1.75（用于桌面客户端和 Tool Runtime）
- **Python** >= 3.10（用于后端服务）
- **protoc** 编译器（用于 gRPC）

### 可选依赖

- **Docker & Docker Compose**（用于容器化部署）
- **Redis**（用于任务队列和配置中心）
- **PostgreSQL**（生产环境数据库，开发可用 SQLite）
- **LiveKit Cloud 账号**（用于语音通话，免费额度）
- **Node.js** >= 18（用于运行 NPX 安装的 MCP 服务器）

### Python 依赖

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

### Rust 依赖（apps/desktop-egui/Cargo.toml）

```toml
[dependencies]
eframe = "0.28"
egui = "0.28"
egui_dock = "0.12"
tokio = { version = "1", features = ["full"] }
reqwest = { version = "0.12", features = ["json", "stream"] }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
anyhow = "1"
chrono = { version = "0.4", features = ["serde"] }
uuid = { version = "1", features = ["v4", "serde"] }
url = "2"
pulldown-cmark = "0.10"
open = "5"  # 用于打开 URL
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

# Rust Tool Runtime（可选，不安装会自动降级到 Python 实现）
cd services/tool-runtime
cargo build --release
cd ../..

# 语音通话依赖（可选）
pip install livekit numpy python-dotenv
```

### 3. 配置环境变量

编辑根目录 `.env`：

```env
# ── LLM 配置 ──────────────────────────────────
MAKIMA_LLM_API_KEY=sk-your-api-key
MAKIMA_LLM_API_BASE=https://api.deepseek.com
MAKIMA_LLM_MODEL=deepseek-chat
MAKIMA_LLM_TEMPERATURE=0.7
MAKIMA_LLM_MAX_TOKENS=4096

# ── 数据库 ──────────────────────────────────────
MAKIMA_DATABASE_URL=sqlite+aiosqlite:///./makima.db

# ── Redis（可选）──────────────────────────────────
MAKIMA_REDIS_URL=redis://localhost:6379/0

# ── LiveKit 语音通话（可选）────────────────────────
LIVEKIT_URL=wss://your-project.livekit.cloud
LIVEKIT_API_KEY=your-api-key
LIVEKIT_API_SECRET=your-api-secret

# ── Fish Audio 语音（TTS + STT）─────────────────────
MAKIMA_FISH_AUDIO_KEY=your-fish-audio-api-key
MAKIMA_FISH_AUDIO_REFERENCE_ID=your-voice-model-id

# ── 其他配置 ──────────────────────────────────────
MAKIMA_DEBUG=false
MAKIMA_API_SECRET_KEY=replace-with-a-long-random-secret
MAKIMA_CLI_USERNAME=your-local-admin
MAKIMA_CLI_PASSWORD=replace-with-a-strong-local-password
```

### 4. 启动后端服务

```bash
# Unix/macOS
./makima-server

# Windows
makima-server.bat

# 或者手动启动
cd apps/backend
python -m makima.app
```

服务启动后访问：
- **API 文档**: http://localhost:8000/docs
- **健康检查**: http://localhost:8000/health

### 5. 启动桌面客户端

```bash
cd apps/desktop-egui
cargo run --release
```

### 6. 启动 CLI 客户端（可选）

```bash
python cli.py
```

### 7. 启动语音通话（可选）

```bash
# 终端 1：启动 Voice Agent Worker
cd services/voice-runtime
python agent.py dev

# 终端 2：启动 CLI 语音客户端
cd services/voice-runtime
python client.py --room makima-voice-room
```

## 使用指南

### MCP Marketplace

在桌面客户端的 Settings 面板中点击 "Marketplace" 标签页：

1. **浏览** — 查看所有可用的 MCP 服务器
2. **搜索** — 按名称或描述搜索
3. **标签过滤** — 按功能标签筛选（database, search, automation 等）
4. **安装** — 点击 "Install" 按钮，填写必要参数（如 API Key）
5. **卸载** — 已安装的服务器显示 "✓ Installed" 标记，点击 "Uninstall" 卸载

安装后，MCP 服务器的工具会自动注册到 Agent，可在对话中使用。

配置文件位置：`.makima/mcp.json`

### 文本对话

在桌面客户端或 CLI 中输入文字与玛奇玛对话：
- 多轮对话
- 工具调用（文件操作、Shell 命令、MCP 工具等）
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

编辑 `.makima/modes.yaml`：

```yaml
modes:
  - slug: code
    name: 🛠️ Code
    model: deepseek-chat
    temperature: 0.0
    tool_groups: [read, write, command, network]
    
  - slug: architect
    name: 🏗️ Architect
    model: gpt-4o
    api_base: https://api.openai.com/v1
    api_key: sk-xxx
    temperature: 0.1
    tool_groups: [read]
```

### 自定义人设

编辑 `.makima/persona.yaml`：

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

## API 端点

### 认证

```bash
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

### MCP Marketplace

```bash
# 列出所有可安装项
GET /api/marketplace/items

# 获取单个详情
GET /api/marketplace/items/{id}

# 获取所有标签
GET /api/marketplace/tags

# 安装 MCP 服务器
POST /api/marketplace/install
{
  "item_id": "brave-search",
  "target": "global",
  "selected_method_index": 0,
  "parameters": {
    "BRAVE_API_KEY": "your_api_key"
  }
}

# 卸载 MCP 服务器
POST /api/marketplace/uninstall
{
  "item_id": "brave-search",
  "target": "global"
}

# 获取已安装列表
GET /api/marketplace/installed

# 检查安装状态
GET /api/marketplace/items/{id}/installed
```

### 会话

```bash
POST /api/sessions
Authorization: Bearer <token>

POST /api/sessions/{session_id}/messages
{
  "content": "你好，玛奇玛"
}

GET /api/sessions
```

### 模式

```bash
GET /api/modes

POST /api/sessions/{session_id}/mode
{
  "mode_slug": "code"
}
```

### 人设

```bash
GET /api/persona

PUT /api/persona
{
  "name": "Makima",
  "personality": "..."
}

POST /api/persona/reload
```

### 检查点

```bash
POST /api/checkpoints
{
  "session_id": "xxx",
  "label": "before-refactor"
}

GET /api/checkpoints?session_id=xxx

POST /api/checkpoints/{checkpoint_id}/restore
```

### 审批

```bash
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

- `apps/desktop-egui/` 是 Rust/egui 桌面客户端，提供完整的 GUI 界面
- `apps/backend/` 是核心单体服务，集中实现了 API、编排、记忆、知识库、MCP 市场等全部功能
- `services/tool-runtime/` 是独立的 Rust gRPC 微服务，提供高性能和安全沙箱的工具执行能力
- `services/voice-runtime/` 是语音通话服务，基于 LiveKit RTC + Fish Audio
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
| Phase 10 | Rust/egui 桌面客户端 | ✅ |
| Phase 11 | MCP Marketplace | ✅ |

详细路线见：[Agent 后端路线文档](docs/architecture/agent-backend-roadmap.md)

## 常见问题

### Q: Rust Tool Runtime 必须安装吗？

A: 不是必须的。如果不安装，系统会自动降级到 Python 实现，功能完全相同，只是性能稍低。

### Q: 如何更换 LLM 模型？

A: 修改根目录 `.env` 中的 `MAKIMA_LLM_MODEL` 和 `MAKIMA_LLM_API_BASE`。支持
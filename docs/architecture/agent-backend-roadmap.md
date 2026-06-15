# Makima Agent Backend Roadmap

> **最后更新**: 2025-06-15  
> **当前状态**: Phase 0-4 已完成 ✅

## 1. 目标

这个项目的目标不是单纯做一个聊天机器人，而是做一个可持续扩展的 Agent 后端：

- 能编排多步任务
- 能调用工具并记录执行过程
- 能沉淀长期记忆
- 能接入知识库与文档检索
- 能支持后续的客户端形态，包括 Unity 拟人化前端

当前已确定的核心参考栈：

- `LangGraph` 作为主编排框架
- `OpenHands` 作为任务执行与工具运行参考
- `Mem0` 作为长期记忆参考

## 2. 总体原则

1. 后端与前端解耦，Agent 核心不依赖具体 UI。
2. 编排、执行、记忆、知识库、权限、观测分层实现。
3. 先做可运行 MVP，再逐步补齐产品化能力。
4. 参考仓库优先用于学习架构和实现模式，不直接把外部项目耦合进主代码。

## 3. 仓库分工

### 3.1 主工程

主工程目录负责你自己的实现：

- `apps/backend/`：对外 API、会话、用户、任务入口
- `services/agent-orchestrator/`：Agent 流程编排
- `services/tool-runtime/`：工具执行层
- `services/memory-service/`：记忆封装
- `services/knowledge-service/`：RAG / 文档检索
- `packages/common/`：公共工具与基础类型
- `packages/schemas/`：事件协议、DTO、接口 schema
- `packages/clients/`：模型与外部系统客户端封装

### 3.2 外部依赖

外部依赖源码统一放在 `external/` 下：

- `external/langgraph/`
- `external/openhands/`
- `external/mem0/`

它们的作用是：

- 作为源码参考和调试
- 实际开发通过 pip install 直接依赖
- 必要时可本地联调或 fork 修改

## 4. 目标架构

```text
Client / UI
   |
   v
API Gateway / Session Service
   |
   v
Agent Orchestrator (LangGraph)
   |
   +--> Tool Runtime (OpenHands style)
   |
   +--> Memory Service (Mem0 style)
   |
   +--> Knowledge Service (RAG / document retrieval)
   |
   v
Storage / Queue / Observability / Safety
```

### 4.1 API 层

负责：

- 用户登录与鉴权
- 会话管理
- 消息收发
- Agent 任务创建与查询

### 4.2 编排层

负责：

- 意图识别后的流程分发
- 多步任务编排
- 中断与恢复
- 人工确认节点
- 流式状态输出

### 4.3 工具执行层

负责：

- 文件读写
- Git 操作
- 浏览器自动化
- 命令执行
- 第三方 API 调用

### 4.4 记忆层

负责：

- 用户偏好
- 历史会话摘要
- 长期上下文
- 任务相关知识沉淀

### 4.5 知识层

负责：

- 文档摄取
- 分块与索引
- 检索增强
- 项目知识回答

### 4.6 安全与观测层

负责：

- 工具权限控制
- 高风险操作确认
- 审计日志
- 链路追踪
- 任务回放

## 5. 实现阶段

### Phase 0: 工程骨架 ✅

目标：

- 目录结构建立完成
- 参考仓库拉取完成
- 基础配置文件到位

产出：

- `README.md` ✅
- `.env.example` ✅
- `docker-compose.yml` ✅
- `Makefile` ✅
- `.gitmodules` ✅
- 目录结构完整 ✅

验收标准：

- ✅ 仓库结构清晰
- ✅ 参考代码可单独打开
- ✅ 主工程没有和参考仓库强耦合

### Phase 1: 最小 Agent 后端 ✅

目标：

- 提供一个可访问的后端 API
- 能发起一次 Agent 任务
- 能返回流式过程与最终结果

产出：

- ✅ FastAPI 应用 (`apps/backend/src/makima/app.py`)
- ✅ JWT 认证系统 (`auth.py`)
- ✅ 会话模型与 API (`routes/sessions.py`)
- ✅ 用户认证 API (`routes/auth.py`)
- ✅ Agent 任务模型 (`models.py`)
- ✅ SSE 流式输出 (`routes/tasks.py`)
- ✅ LangGraph 编排 (`orchestrator/graph.py`)
- ✅ 事件协议 (`packages/schemas/src/makima_schemas/events.py`)
- ✅ API DTO (`packages/schemas/src/makima_schemas/api.py`)
- ✅ 配置管理 (`packages/common/src/makima_common/config.py`)
- ✅ 结构化日志 (`packages/common/src/makima_common/logging.py`)
- ✅ LLM 客户端封装 (`clients/llm.py`)

验收标准：

- ✅ 用户发消息后可以进入 Agent 流程
- ✅ Agent 至少能完成一个简单任务
- ✅ 过程日志可追踪

### Phase 2: 工具执行 ✅

目标：

- 接入可控工具集
- 让 Agent 真正开始做事

优先工具：

- ✅ 文件系统读写 (`tools/file_tool.py`)
- ✅ HTTP 请求 (`tools/http_tool.py`)
- ✅ Shell 命令 (`tools/shell_tool.py`)
- ✅ 工具注册表 (`tools/registry.py`)

安全特性：

- ✅ 路径遍历防护
- ✅ 危险命令黑名单
- ✅ 内网地址请求拦截
- ✅ 命令执行超时控制

验收标准：

- ✅ 工具调用有统一协议
- ✅ 工具结果可回传编排层
- ✅ 风险操作可拦截或要求确认

### Phase 3: 记忆与知识库 ✅

目标：

- 引入长期记忆
- 引入项目知识检索

产出：

- ✅ Mem0 客户端封装 (`clients/memory.py`)
- ✅ Memory Service 层 (`memory/service.py`)
- ✅ Memory API 路由 (`routes/memory.py`)
- ✅ Knowledge 数据模型 (`knowledge/models.py`)
- ✅ 文档摄取管道 (`knowledge/ingest.py`)
- ✅ RAG 检索 (`knowledge/retriever.py`)
- ✅ Knowledge API 路由 (`routes/knowledge.py`)
- ✅ 编排层集成记忆和知识 (`orchestrator/graph.py`, `orchestrator/runner.py`)

验收标准：

- ✅ Agent 能记住用户偏好（通过 Mem0）
- ✅ Agent 能基于文档回答问题（通过 RAG）
- ✅ 多轮对话上下文更稳定（记忆自动注入 system prompt）

### Phase 4: 产品化能力 ✅

目标：

- 把系统从"能用"推进到"可长期使用"

产出：

- ✅ RBAC 权限体系 (`security/rbac.py`) — 角色分级、权限声明式校验
- ✅ 审计日志系统 (`audit/models.py`, `audit/service.py`, `routes/audit.py`)
- ✅ Celery 任务队列 (`queue/celery_app.py`, `queue/tasks.py`) — 文档处理、记忆提取、Agent 异步执行
- ✅ 中间件层 (`middleware.py`) — RequestID 追踪、请求超时、异步重试
- ✅ Prometheus 指标 (`observability/metrics.py`, `routes/metrics.py`) — HTTP/Agent/Tool/Queue 全链路埋点
- ✅ OpenTelemetry 分布式追踪 (`observability/tracing.py`) — FastAPI + SQLAlchemy 自动 instrumentation
- ✅ 配置中心 (`config_center/service.py`, `routes/admin.py`) — Redis 后端、本地缓存、动态热更新
- ✅ 管理路由 (`routes/admin.py`) — 配置管理、系统健康检查

验收标准：

- ✅ 失败可定位（RequestID + 审计日志 + OpenTelemetry 链路追踪）
- ✅ 高风险操作可追溯（全量审计日志，含用户/资源/时间/IP 信息）
- ✅ 长任务可恢复（Celery 异步执行 + 自动重试 + 指数退避）

### Phase 5: 客户端接入 🔲

目标：

- 给后续 Unity 拟人前端接入稳定接口

产出：

- 🔲 事件协议
- 🔲 流式状态协议
- 🔲 语音与动作事件协议

验收标准：

- 🔲 客户端只负责展示与输入输出
- 🔲 业务逻辑不耦合到前端

## 6. 推荐技术选型

### 6.1 后端主栈

- Python 3.12
- FastAPI
- LangGraph
- PostgreSQL
- Redis
- Celery 或其他任务队列方案

### 6.2 记忆与知识

- Mem0
- LlamaIndex
- pgvector 或向量数据库

### 6.3 工具执行

- OpenHands 风格执行器
- Docker / 沙箱隔离
- 标准化 tool schema

### 6.4 观测与安全

- 结构化日志
- OpenTelemetry
- 审计表
- 权限白名单

## 7. 目录与职责对应

```text
apps/backend/                # 对外服务入口 ✅
services/api-gateway/        # API 和会话层
services/agent-orchestrator/ # LangGraph 编排
services/tool-runtime/       # 工具执行
services/memory-service/     # 记忆封装
services/knowledge-service/  # RAG 与检索
packages/common/             # 通用代码 ✅
packages/schemas/            # 协议与结构定义 ✅
packages/clients/            # 外部依赖客户端
external/*                   # 外部依赖源码
```

## 8. 近期执行顺序

1. ~~完成 `apps/backend` 的最小 API~~ ✅
2. ~~把 LangGraph 作为编排骨架接起来~~ ✅
3. ~~接入最小工具执行层~~ ✅
4. ~~引入 Mem0 作为记忆层~~ ✅
5. ~~加入文档检索服务~~ ✅
6. ~~补权限、队列、日志、回放~~ ✅

## 9. 这套路线的判断标准

如果下面几项都成立，就说明路线正确：

- 后端能持续接入更多工具
- Agent 的流程是可观测的
- 记忆和知识是可扩展的
- 前端可以随时替换，不影响后端核心
- 后续接 Unity 只需要对接协议，不需要重写后端
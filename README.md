# Makima Agent

Makima Agent 是一个面向“成熟 Agent 能力 + 拟人化表现层”方向的后端工程起点。

当前项目的核心思路是：

- 以 `LangGraph` 作为 Agent 编排骨架
- 以 `OpenHands` 作为任务执行与工具运行参考
- 以 `Mem0` 作为长期记忆参考
- 后续再接 Unity / 其他前端形态

## 目录结构

```text
AgentProject/
├─ apps/
│  └─ backend/
├─ services/
│  ├─ api-gateway/
│  ├─ agent-orchestrator/
│  ├─ tool-runtime/
│  ├─ memory-service/
│  └─ knowledge-service/
├─ packages/
│  ├─ common/
│  ├─ schemas/
│  └─ clients/
├─ integrations/
│  ├─ langgraph/
│  ├─ openhands/
│  └─ mem0/
├─ docs/
│  ├─ architecture/
│  ├─ api/
│  └─ decisions/
├─ infra/
└─ README.md
```

## 参考仓库

这三个目录用于放外部参考仓库，方便对照实现：

- `integrations/langgraph/`
- `integrations/openhands/`
- `integrations/mem0/`

## 实现路线

详细达成路径见：

- [Agent 后端路线文档](docs/architecture/agent-backend-roadmap.md)

## 推荐推进顺序

1. 先完成 `apps/backend` 的最小 API
2. 再接 `services/agent-orchestrator`
3. 接入 `services/tool-runtime`
4. 再接 `services/memory-service`
5. 最后补 `services/knowledge-service`

## 当前状态

- 项目骨架已建立
- 参考仓库目录已预留
- 路线文档已创建

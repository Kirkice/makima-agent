"""Agent runner — executes the LangGraph workflow with memory and knowledge."""

from __future__ import annotations

import time
from collections.abc import AsyncGenerator
from uuid import UUID

from langchain_core.messages import HumanMessage

from makima.orchestrator.graph import build_graph
from makima.memory.service import MemoryService
from makima.knowledge.retriever import retrieve, format_context_for_prompt
from makima_schemas.events import AgentEvent, AgentEventType
from makima_common.config import get_settings
from makima_common.logging import get_logger

logger = get_logger(__name__)


async def run_agent(
    input_text: str,
    session_id: str,
    user_id: str,
    db: object | None = None,
) -> AsyncGenerator[AgentEvent, None]:
    """Run the agent with the given input, integrating memory and knowledge.

    Args:
        input_text: User input text.
        session_id: Session ID for checkpointing.
        user_id: User ID for memory and knowledge scoping.
        db: Optional database session for knowledge retrieval.

    Yields:
        AgentEvent instances representing agent execution events.
    """
    settings = get_settings()
    step = 0
    start_time = time.time()

    # Emit thinking event
    yield AgentEvent(
        type=AgentEventType.THINKING,
        data={"input": input_text},
        timestamp=start_time,
        step=step,
    )

    # ── Recall memories ────────────────────────────────────────────────
    memory_context = ""
    if settings.memory_enabled:
        try:
            memory_service = MemoryService()
            if memory_service.available:
                memories = memory_service.recall(
                    query=input_text, user_id=user_id, limit=5
                )
                memory_context = memory_service.format_memories_for_prompt(memories)
                if memories:
                    step += 1
                    yield AgentEvent(
                        type=AgentEventType.THINKING,
                        data={
                            "phase": "memory_recall",
                            "count": len(memories),
                        },
                        timestamp=time.time(),
                        step=step,
                    )
        except Exception as e:
            logger.warning("Memory recall failed", error=str(e))

    # ── Retrieve knowledge ─────────────────────────────────────────────
    knowledge_context = ""
    if settings.knowledge_enabled and db is not None:
        try:
            results = await retrieve(
                db=db,
                query=input_text,
                user_id=UUID(user_id),
                top_k=settings.rag_top_k,
            )
            knowledge_context = format_context_for_prompt(results)
            if results:
                step += 1
                yield AgentEvent(
                    type=AgentEventType.THINKING,
                    data={
                        "phase": "knowledge_retrieval",
                        "count": len(results),
                    },
                    timestamp=time.time(),
                    step=step,
                )
        except Exception as e:
            logger.warning("Knowledge retrieval failed", error=str(e))

    # ── Build and run graph ────────────────────────────────────────────
    graph = build_graph(
        memory_context=memory_context,
        knowledge_context=knowledge_context,
    )
    config = {"configurable": {"thread_id": session_id}}

    initial_state = {
        "messages": [HumanMessage(content=input_text)],
        "user_id": user_id,
        "session_id": session_id,
        "context": {},
    }

    try:
        # Use astream to capture state updates including tool calls/results
        async for chunk in graph.astream(initial_state, config=config, stream_mode="updates"):
            step += 1
            # chunk is a dict of {node_name: state_update}
            for node_name, state_update in chunk.items():
                messages = state_update.get("messages", [])
                for msg in messages:
                    # Tool calls from agent node
                    if hasattr(msg, "tool_calls") and msg.tool_calls:
                        for tc in msg.tool_calls:
                            yield AgentEvent(
                                type=AgentEventType.TOOL_CALL,
                                data={"tool": tc.get("name", "unknown"), "input": tc.get("args", {})},
                                timestamp=time.time(),
                                step=step,
                            )
                    # Tool results from tools node
                    elif hasattr(msg, "name") and node_name == "tools":
                        yield AgentEvent(
                            type=AgentEventType.TOOL_RESULT,
                            data={"tool": msg.name or "unknown", "output": str(msg.content)},
                            timestamp=time.time(),
                            step=step,
                        )
                    # Final AI message (no tool calls)
                    elif hasattr(msg, "content") and msg.content and node_name == "agent":
                        if not (hasattr(msg, "tool_calls") and msg.tool_calls):
                            yield AgentEvent(
                                type=AgentEventType.MESSAGE,
                                data={"content": str(msg.content)},
                                timestamp=time.time(),
                                step=step,
                            )

    except Exception as e:
        logger.error("Agent execution failed", error=str(e), session_id=session_id)
        yield AgentEvent(
            type=AgentEventType.ERROR,
            data={"error": str(e)},
            timestamp=time.time(),
            step=step,
        )
        raise

    # ── Store conversation to memory ───────────────────────────────────
    if settings.memory_enabled:
        try:
            memory_service = MemoryService()
            if memory_service.available:
                messages = [
                    {"role": "user", "content": input_text},
                ]
                memory_service.store_conversation(
                    messages=messages,
                    user_id=user_id,
                    session_id=session_id,
                )
        except Exception as e:
            logger.warning("Memory store failed", error=str(e))
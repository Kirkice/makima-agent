"""Agent runner — executes the LangGraph workflow."""

from __future__ import annotations

import time
from collections.abc import AsyncGenerator
from uuid import UUID

from langchain_core.messages import HumanMessage

from makima.models import Session
from makima.orchestrator.graph import build_graph
from makima_schemas.events import AgentEvent, AgentEventType
from makima_common.logging import get_logger

logger = get_logger(__name__)


async def run_agent(
    input_text: str,
    session_id: str,
) -> AsyncGenerator[AgentEvent, None]:
    """
    Run the agent with the given input.

    Args:
        input_text: User input text
        session_id: Session ID for checkpointing

    Yields:
        AgentEvent instances representing agent execution events
    """
    graph = build_graph()
    config = {"configurable": {"thread_id": session_id}}

    step = 0
    start_time = time.time()

    # Emit thinking event
    yield AgentEvent(
        type=AgentEventType.THINKING,
        data={"input": input_text},
        timestamp=start_time,
        step=step,
    )

    try:
        async for event in graph.astream_events(
            {"messages": [HumanMessage(content=input_text)]},
            config=config,
            version="v2",
        ):
            step += 1
            event_kind = event.get("event")

            if event_kind == "on_chat_model_stream":
                # LLM streaming output
                chunk = event.get("data", {}).get("chunk")
                if chunk and chunk.content:
                    yield AgentEvent(
                        type=AgentEventType.MESSAGE,
                        data={"content": chunk.content, "partial": True},
                        timestamp=time.time(),
                        step=step,
                    )

            elif event_kind == "on_tool_start":
                # Tool invocation
                tool_name = event.get("name", "unknown")
                tool_input = event.get("data", {}).get("input", {})
                yield AgentEvent(
                    type=AgentEventType.TOOL_CALL,
                    data={"tool": tool_name, "input": tool_input},
                    timestamp=time.time(),
                    step=step,
                )

            elif event_kind == "on_tool_end":
                # Tool result
                tool_name = event.get("name", "unknown")
                tool_output = event.get("data", {}).get("output", "")
                yield AgentEvent(
                    type=AgentEventType.TOOL_RESULT,
                    data={"tool": tool_name, "output": str(tool_output)},
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
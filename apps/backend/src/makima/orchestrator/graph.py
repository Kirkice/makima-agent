"""LangGraph agent graph definition with mode system and prompt engine integration."""

from __future__ import annotations

from typing import Annotated, Any, TypedDict

from langchain_core.messages import BaseMessage, SystemMessage
from langgraph.graph import END, StateGraph
from langgraph.graph.message import add_messages
from langgraph.prebuilt import ToolNode

from makima.clients.llm import get_chat_model_for_mode, get_chat_model_with_override
from makima.modes.registry import get_default_mode, get_mode
from makima.modes.tool_groups import get_tools_for_configs
from makima.prompts.engine import PromptEngine
from makima.tools.registry import get_tools, get_tools_by_names
from makima_schemas import ModeConfig, Persona
from makima_common.config import get_settings
from makima_common.logging import get_logger

logger = get_logger(__name__)

# Global prompt engine instance
_prompt_engine = PromptEngine()


class AgentState(TypedDict):
    """State carried through the LangGraph execution."""

    messages: Annotated[list[BaseMessage], add_messages]
    user_id: str
    session_id: str
    context: dict[str, Any]


def _get_tools_for_mode(mode: ModeConfig) -> list:
    """Get tools filtered by mode's tool groups."""
    allowed_tool_names = get_tools_for_configs(mode.tool_groups)
    if allowed_tool_names:
        return get_tools_by_names(allowed_tool_names)
    # If mode has no tool groups (e.g., chat mode), return empty list
    return []


def build_graph(
    mode: ModeConfig | None = None,
    persona: Persona | None = None,
    memory_context: str = "",
    knowledge_context: str = "",
    model_override: dict | None = None,
) -> Any:
    """Build and compile the agent graph.

    Args:
        mode: Optional mode configuration. Uses default mode if not provided.
        persona: Optional persona configuration for personality injection.
        memory_context: Pre-retrieved memory context to inject.
        knowledge_context: Pre-retrieved knowledge context to inject.

    Returns:
        Compiled LangGraph graph.
    """
    # Get mode (default to 'code' if not specified)
    if mode is None:
        mode = get_mode("code") or get_default_mode()

    # Get LLM with mode-specific configuration + optional client override
    if model_override:
        llm = get_chat_model_with_override(
            mode,
            model_name=model_override.get("model"),
            api_key=model_override.get("api_key"),
            base_url=model_override.get("base_url"),
            temperature=model_override.get("temperature"),
        )
        logger.debug("Using model with override", mode=mode.slug, model=llm.model_name)
    else:
        llm = get_chat_model_for_mode(mode)
        logger.debug("Using model for mode", mode=mode.slug, model=llm.model_name)

    # Get tools filtered by mode
    tools = _get_tools_for_mode(mode)

    # Build system prompt using PromptEngine
    system_content = _prompt_engine.build_system_prompt_simple(
        mode=mode,
        memory_context=memory_context if memory_context else None,
        knowledge_context=knowledge_context if knowledge_context else None,
        persona=persona,
    )

    logger.debug(
        "Building graph",
        mode=mode.slug,
        num_tools=len(tools),
        prompt_length=len(system_content),
    )

    # Bind tools to LLM
    if tools:
        llm_with_tools = llm.bind_tools(tools)
    else:
        llm_with_tools = llm

    async def agent_node(state: AgentState) -> dict:
        """Agent node: call LLM to generate response or tool calls."""
        messages = state["messages"]

        # Prepend system message
        full_messages = [SystemMessage(content=system_content)] + messages

        # Use astream to enable on_chat_model_stream events in astream_events
        response = None
        async for chunk in llm_with_tools.astream(full_messages):
            if response is None:
                response = chunk
            else:
                response += chunk  # accumulate chunks

        return {"messages": [response]}

    def should_continue(state: AgentState) -> str:
        """Determine whether to continue with tools or end."""
        last_message = state["messages"][-1]
        if hasattr(last_message, "tool_calls") and last_message.tool_calls:
            return "tools"
        return END

    # Build graph
    workflow = StateGraph(AgentState)

    # Add nodes
    workflow.add_node("agent", agent_node)
    if tools:
        workflow.add_node("tools", ToolNode(tools))

    # Set entry point
    workflow.set_entry_point("agent")

    # Add edges
    if tools:
        workflow.add_conditional_edges(
            "agent", should_continue, {"tools": "tools", END: END}
        )
        workflow.add_edge("tools", "agent")
    else:
        workflow.add_edge("agent", END)

    return workflow.compile()
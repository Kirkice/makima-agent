"""LangGraph agent graph definition with memory and knowledge integration."""

from __future__ import annotations

from typing import Annotated, Any, TypedDict

from langchain_core.messages import BaseMessage, SystemMessage
from langgraph.graph import END, StateGraph
from langgraph.graph.message import add_messages
from langgraph.prebuilt import ToolNode

from makima.clients.llm import get_chat_model
from makima_common.config import get_settings
from makima_common.logging import get_logger
from makima.memory.service import MemoryService
from makima.tools.registry import get_tools

logger = get_logger(__name__)

SYSTEM_PROMPT = """You are Makima, a helpful AI assistant with access to various tools.

You can:
- Read, write, and list files in the workspace
- Execute shell commands
- Make HTTP requests
- Search your knowledge base and memory for relevant information

When using tools, be precise and efficient. Always explain what you're doing.
If you encounter an error, try to diagnose and fix it.
"""


class AgentState(TypedDict):
    """State carried through the LangGraph execution."""

    messages: Annotated[list[BaseMessage], add_messages]
    user_id: str
    session_id: str
    context: dict[str, Any]


def build_graph(
    memory_service: MemoryService | None = None,
    knowledge_context: str = "",
    memory_context: str = "",
) -> Any:
    """Build and compile the agent graph.

    Args:
        memory_service: Optional memory service for recall/storage.
        knowledge_context: Pre-retrieved knowledge context to inject.
        memory_context: Pre-retrieved memory context to inject.

    Returns:
        Compiled LangGraph graph.
    """
    settings = get_settings()
    llm = get_chat_model()
    tools = get_tools()

    # Build system prompt with context
    system_content = SYSTEM_PROMPT
    if memory_context:
        system_content += f"\n\n{memory_context}"
    if knowledge_context:
        system_content += f"\n\n{knowledge_context}"

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

        response = await llm_with_tools.ainvoke(full_messages)
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
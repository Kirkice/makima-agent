"""LangGraph agent graph definition."""

from __future__ import annotations

from typing import Annotated, TypedDict

from langchain_core.messages import BaseMessage
from langgraph.graph import END, StateGraph
from langgraph.graph.message import add_messages
from langgraph.prebuilt import ToolNode

from makima.clients.llm import get_chat_model
from makima_common.config import get_settings
from makima_common.logging import get_logger
from makima.tools.registry import get_tools

logger = get_logger(__name__)


class AgentState(TypedDict):
    """State carried through the LangGraph execution."""

    messages: Annotated[list[BaseMessage], add_messages]


def build_graph() -> StateGraph:
    """Build and compile the agent graph."""
    settings = get_settings()
    llm = get_chat_model()
    tools = get_tools()

    # Bind tools to LLM
    if tools:
        llm_with_tools = llm.bind_tools(tools)
    else:
        llm_with_tools = llm

    async def agent_node(state: AgentState) -> dict:
        """Agent node: call LLM to generate response or tool calls."""
        messages = state["messages"]
        response = await llm_with_tools.ainvoke(messages)
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
        workflow.add_conditional_edges("agent", should_continue, {"tools": "tools", END: END})
        workflow.add_edge("tools", "agent")
    else:
        workflow.add_edge("agent", END)

    return workflow.compile()
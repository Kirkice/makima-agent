"""Tool registry — manages all available tools."""

from __future__ import annotations

from langchain_core.tools import BaseTool

from makima.tools.file_tool import read_file, write_file, list_directory
from makima.tools.shell_tool import execute_shell
from makima.tools.http_tool import http_request
from makima.tools.switch_mode import switch_mode
from makima_common.logging import get_logger

logger = get_logger(__name__)

# All available tools
_AVAILABLE_TOOLS: list[BaseTool] = [
    read_file,
    write_file,
    list_directory,
    execute_shell,
    http_request,
    switch_mode,
]

# Tool name to tool instance mapping for quick lookup
_TOOL_MAP: dict[str, BaseTool] = {tool.name: tool for tool in _AVAILABLE_TOOLS}


def get_tools() -> list[BaseTool]:
    """Return all registered tools."""
    return _AVAILABLE_TOOLS


def get_tool_by_name(name: str) -> BaseTool | None:
    """Get a tool by name."""
    return _TOOL_MAP.get(name)


def get_tools_by_names(names: list[str]) -> list[BaseTool]:
    """Get multiple tools by their names.

    Args:
        names: List of tool names to retrieve

    Returns:
        List of BaseTool instances for the given names
    """
    tools = []
    for name in names:
        tool = _TOOL_MAP.get(name)
        if tool is not None:
            tools.append(tool)
        else:
            logger.warning("Tool not found in registry", tool_name=name)
    return tools


def get_all_tool_names() -> list[str]:
    """Get all registered tool names."""
    return list(_TOOL_MAP.keys())
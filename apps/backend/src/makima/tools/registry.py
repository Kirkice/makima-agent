"""Tool registry — manages all available tools."""

from __future__ import annotations

from langchain_core.tools import BaseTool

from makima.tools.file_tool import read_file, write_file, list_directory
from makima.tools.shell_tool import execute_shell
from makima.tools.http_tool import http_request
from makima_common.logging import get_logger

logger = get_logger(__name__)

# All available tools
_AVAILABLE_TOOLS: list[BaseTool] = [
    read_file,
    write_file,
    list_directory,
    execute_shell,
    http_request,
]


def get_tools() -> list[BaseTool]:
    """Return all registered tools."""
    return _AVAILABLE_TOOLS


def get_tool_by_name(name: str) -> BaseTool | None:
    """Get a tool by name."""
    for tool in _AVAILABLE_TOOLS:
        if tool.name == name:
            return tool
    return None
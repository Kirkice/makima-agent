"""Capabilities section for system prompt."""

from makima.prompts.types import PromptContext


def build_capabilities_section(ctx: PromptContext) -> str:
    """Build the capabilities section based on current mode and tools."""
    mode = ctx.mode
    lines = ["# Your Capabilities\n"]

    # Tool-based capabilities
    tool_groups = {tg.group.value for tg in mode.tool_groups}

    if "read" in tool_groups:
        lines.append("- **Read files and search code**: You can read, list, and search files in the workspace")
    if "write" in tool_groups:
        lines.append("- **Write and edit files**: You can create and modify files")
    if "command" in tool_groups:
        lines.append("- **Execute commands**: You can run shell commands")
    if "network" in tool_groups:
        lines.append("- **Make HTTP requests**: You can call external APIs")
    if "mcp" in tool_groups:
        lines.append("- **Use MCP tools**: You can access MCP (Model Context Protocol) tools")
    if "system" in tool_groups:
        lines.append("- **Switch modes**: You can request to switch to a different mode when needed")

    # Memory capability
    lines.append("- **Long-term memory**: You remember user preferences and past conversations")

    # Knowledge capability
    lines.append("- **Knowledge base**: You can retrieve relevant information from the knowledge base")

    return "\n".join(lines)
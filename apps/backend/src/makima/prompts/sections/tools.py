"""Tools section for system prompt."""

from makima.prompts.types import ToolDescription


def build_tools_section(tools: list[ToolDescription]) -> str:
    """Build the available tools section."""
    if not tools:
        return ""

    lines = ["# Available Tools\n", "You have access to the following tools:\n"]

    for tool in tools:
        # Tool header
        risk_indicator = ""
        if tool.risk_level == "high":
            risk_indicator = " ⚠️ [HIGH RISK]"
        elif tool.risk_level == "medium":
            risk_indicator = " ⚡ [MEDIUM RISK]"

        lines.append(f"## {tool.name}{risk_indicator}")
        lines.append(f"{tool.description}\n")

        # Parameters
        if tool.parameters:
            lines.append("**Parameters:**")
            for param_name, param_info in tool.parameters.items():
                required = param_info.get("required", False)
                param_type = param_info.get("type", "any")
                param_desc = param_info.get("description", "")
                req_marker = " (required)" if required else ""
                lines.append(f"- `{param_name}` ({param_type}){req_marker}: {param_desc}")
            lines.append("")

        # Approval warning
        if tool.requires_approval:
            lines.append("⚠️ **This tool requires user approval before execution.**\n")

        lines.append("---\n")

    return "\n".join(lines)
"""Tools section for system prompt — enhanced with structured descriptions.

Each tool is rendered with:
- Name and risk level indicator
- Description
- Parameters (JSON Schema style)
- Usage example
- Risk level and approval requirements
"""

from __future__ import annotations

import json
from typing import Any

from makima.prompts.types import ToolDescription


def _format_json_schema(parameters: dict[str, Any]) -> str:
    """Format parameters as a JSON Schema snippet."""
    if not parameters:
        return "No parameters required."

    lines = ["```json", "{"]
    for param_name, param_info in parameters.items():
        param_type = param_info.get("type", "any")
        param_desc = param_info.get("description", "")
        required = param_info.get("required", False)
        req_tag = " [required]" if required else ""
        lines.append(f'  "{param_name}": "<{param_type}>{req_tag}"  // {param_desc}')
    lines.append("}")
    lines.append("```")
    return "\n".join(lines)


def _generate_usage_example(tool: ToolDescription) -> str:
    """Generate a usage example for a tool."""
    if not tool.parameters:
        return f'<{tool.name}></{tool.name}>'

    lines = [f"<{tool.name}>"]
    for param_name, param_info in tool.parameters.items():
        if param_info.get("required", False):
            param_type = param_info.get("type", "string")
            example_value = _get_example_value(param_name, param_type)
            lines.append(f"  <{param_name}>{example_value}</{param_name}>")
    lines.append(f"</{tool.name}>")
    return "\n".join(lines)


def _get_example_value(param_name: str, param_type: str) -> str:
    """Get a reasonable example value based on parameter name and type."""
    # Common parameter patterns
    examples = {
        "path": "/path/to/file",
        "file_path": "/path/to/file",
        "command": "ls -la",
        "url": "https://example.com",
        "content": "file content here",
        "query": "search query",
        "text": "text content",
        "name": "example",
        "directory": "/path/to/dir",
        "mode_slug": "code",
        "reason": "description of why",
    }

    if param_name in examples:
        return examples[param_name]

    # Fallback based on type
    type_examples = {
        "string": "value",
        "int": "42",
        "integer": "42",
        "bool": "true",
        "boolean": "true",
        "float": "1.5",
        "list": "[]",
        "dict": "{}",
    }
    return type_examples.get(param_type, "value")


def _get_risk_description(risk_level: str) -> str:
    """Get human-readable risk description."""
    descriptions = {
        "low": "🟢 Low risk — safe to use freely",
        "medium": "🟡 Medium risk — review before executing",
        "high": "🔴 High risk — requires careful consideration",
    }
    return descriptions.get(risk_level, f"⚪ {risk_level} risk")


def build_tools_section(tools: list[ToolDescription]) -> str:
    """Build the available tools section with enhanced descriptions."""
    if not tools:
        return ""

    lines = [
        "# Available Tools\n",
        "You have access to the following tools. Use them when appropriate to accomplish tasks.\n",
    ]

    for tool in tools:
        # Tool header with risk indicator
        risk_indicator = ""
        if tool.risk_level == "high":
            risk_indicator = " 🔴"
        elif tool.risk_level == "medium":
            risk_indicator = " 🟡"

        lines.append(f"## {tool.name}{risk_indicator}\n")

        # Description
        lines.append(f"{tool.description}\n")

        # Parameters
        lines.append("**Parameters:**")
        lines.append(_format_json_schema(tool.parameters))
        lines.append("")

        # Usage example
        lines.append("**Example:**")
        lines.append("```xml")
        lines.append(_generate_usage_example(tool))
        lines.append("```\n")

        # Risk and approval info
        lines.append(f"**Risk:** {_get_risk_description(tool.risk_level)}")
        if tool.requires_approval:
            lines.append("**Approval:** ⚠️ This tool requires user confirmation before execution.")
        else:
            lines.append("**Approval:** Auto-approved")
        lines.append("")
        lines.append("---\n")

    # Summary table
    lines.append("## Tool Summary\n")
    lines.append("| Tool | Risk | Approval |")
    lines.append("|------|------|----------|")
    for tool in tools:
        risk_emoji = {"low": "🟢", "medium": "🟡", "high": "🔴"}.get(tool.risk_level, "⚪")
        approval = "⚠️ Required" if tool.requires_approval else "✅ Auto"
        lines.append(f"| {tool.name} | {risk_emoji} {tool.risk_level} | {approval} |")
    lines.append("")

    return "\n".join(lines)
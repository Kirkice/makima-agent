"""Tool group to tool mapping."""

from makima_schemas import ToolGroup, ToolGroupConfig

# Mapping from tool groups to tool names
TOOL_GROUP_MAPPING: dict[ToolGroup, list[str]] = {
    ToolGroup.READ: [
        "read_file",
        "list_directory",
    ],
    ToolGroup.WRITE: [
        "write_file",
    ],
    ToolGroup.COMMAND: [
        "execute_shell",
    ],
    ToolGroup.NETWORK: [
        "http_request",
    ],
    ToolGroup.MCP: [],  # MCP tools can be added dynamically
    ToolGroup.SYSTEM: [
        "switch_mode",
    ],
}


def get_tools_for_groups(tool_groups: list[str]) -> list[str]:
    """Get tool names for the given tool groups.

    Args:
        tool_groups: List of tool group names (e.g., ['read', 'write'])

    Returns:
        List of tool names available for those groups
    """
    tools = set()
    for group_name in tool_groups:
        try:
            group = ToolGroup(group_name)
            tools.update(TOOL_GROUP_MAPPING.get(group, []))
        except ValueError:
            pass  # Unknown group, skip
    return sorted(tools)


def get_tools_for_configs(tool_group_configs: list[ToolGroupConfig]) -> list[str]:
    """Get tool names for the given tool group configs.

    Args:
        tool_group_configs: List of ToolGroupConfig objects

    Returns:
        List of tool names available for those configs
    """
    group_names = [cfg.group.value for cfg in tool_group_configs]
    return get_tools_for_groups(group_names)
"""Mode system for multi-mode Agent.

Provides mode registration, lookup, and custom mode loading from YAML.
"""

from makima.modes.builtin import BUILTIN_MODES
from makima.modes.loader import load_all_custom_modes, load_project_modes
from makima.modes.registry import (
    get_default_mode,
    get_mode,
    has_mode,
    list_modes,
    register_mode,
    unregister_mode,
)
from makima.modes.tool_groups import (
    TOOL_GROUP_MAPPING,
    get_tools_for_configs,
    get_tools_for_groups,
)

__all__ = [
    "BUILTIN_MODES",
    "TOOL_GROUP_MAPPING",
    "get_default_mode",
    "get_mode",
    "get_tools_for_configs",
    "get_tools_for_groups",
    "has_mode",
    "list_modes",
    "load_all_custom_modes",
    "load_project_modes",
    "register_mode",
    "unregister_mode",
]
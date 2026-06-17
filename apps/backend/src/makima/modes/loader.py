"""Mode loader — loads custom modes from YAML configuration files."""

from __future__ import annotations

from pathlib import Path

import yaml

from makima.modes.registry import register_mode
from makima_schemas import ModeConfig, ToolGroup, ToolGroupConfig
from makima_common.logging import get_logger

logger = get_logger(__name__)

# Default configuration directory name
CONFIG_DIR_NAME = ".makima"
MODES_FILE_NAME = "modes.yaml"


def _parse_tool_group(raw: dict | str) -> ToolGroupConfig:
    """Parse a tool group from YAML raw data.

    Args:
        raw: Either a string (group name) or a dict with group config.

    Returns:
        ToolGroupConfig instance.
    """
    if isinstance(raw, str):
        return ToolGroupConfig(group=ToolGroup(raw))

    group_name = raw.get("group", "")
    return ToolGroupConfig(
        group=ToolGroup(group_name),
        file_regex=raw.get("file_regex"),
        auto_approve=raw.get("auto_approve", True),
    )


def _parse_mode(raw: dict, source: str = "project") -> ModeConfig | None:
    """Parse a mode from YAML raw data.

    Args:
        raw: Dictionary from YAML parsing.
        source: Source type ('project' or 'custom').

    Returns:
        ModeConfig instance, or None if parsing fails.
    """
    try:
        slug = raw.get("slug", "")
        if not slug:
            logger.warning("Skipping mode without slug", raw=raw)
            return None

        # Parse tool groups
        tool_groups = []
        raw_groups = raw.get("tool_groups", [])
        for group_raw in raw_groups:
            tg = _parse_tool_group(group_raw)
            tool_groups.append(tg)

        # Build kwargs, only including optional LLM fields if they exist
        kwargs = {
            "slug": slug,
            "name": raw.get("name", slug),
            "role_definition": raw.get("role_definition", ""),
            "when_to_use": raw.get("when_to_use"),
            "description": raw.get("description"),
            "custom_instructions": raw.get("custom_instructions"),
            "tool_groups": tool_groups,
            "max_steps": raw.get("max_steps", 30),
            "temperature": raw.get("temperature", 0.0),
            "source": source,
        }

        # Optional LLM configuration (only add if present in YAML)
        if "model" in raw and raw["model"] is not None:
            kwargs["model"] = raw["model"]
        if "api_base" in raw and raw["api_base"] is not None:
            kwargs["api_base"] = raw["api_base"]
        if "api_key" in raw and raw["api_key"] is not None:
            kwargs["api_key"] = raw["api_key"]

        return ModeConfig(**kwargs)
    except Exception as e:
        logger.error("Failed to parse mode", error=str(e), raw=raw)
        return None


def load_modes_from_file(path: Path) -> list[ModeConfig]:
    """Load modes from a YAML file.

    Args:
        path: Path to the YAML file.

    Returns:
        List of loaded ModeConfig objects.
    """
    if not path.exists():
        logger.debug("Mode config file not found", path=str(path))
        return []

    try:
        with open(path, "r", encoding="utf-8") as f:
            data = yaml.safe_load(f)

        if not data or "modes" not in data:
            logger.debug("No modes found in config file", path=str(path))
            return []

        modes = []
        for raw_mode in data["modes"]:
            mode = _parse_mode(raw_mode, source="project")
            if mode:
                modes.append(mode)
                register_mode(mode)

        logger.info(
            "Loaded custom modes",
            path=str(path),
            count=len(modes),
        )
        return modes

    except Exception as e:
        logger.error("Failed to load modes from file", path=str(path), error=str(e))
        return []


def load_project_modes(project_root: Path | None = None) -> list[ModeConfig]:
    """Load custom modes from the project's .makima/modes.yaml.

    Args:
        project_root: Root directory of the project. Defaults to current directory.

    Returns:
        List of loaded ModeConfig objects.
    """
    if project_root is None:
        project_root = Path.cwd()

    config_path = project_root / CONFIG_DIR_NAME / MODES_FILE_NAME
    return load_modes_from_file(config_path)


def load_all_custom_modes(project_root: Path | None = None) -> list[ModeConfig]:
    """Load all custom modes from project and user directories.

    Args:
        project_root: Root directory of the project.

    Returns:
        List of all loaded custom ModeConfig objects.
    """
    modes: list[ModeConfig] = []

    # Load project-level modes
    modes.extend(load_project_modes(project_root))

    # Load user-level modes from home directory
    user_config = Path.home() / CONFIG_DIR_NAME / MODES_FILE_NAME
    if user_config.exists():
        user_modes = load_modes_from_file(user_config)
        for m in user_modes:
            m.source = "custom"
        modes.extend(user_modes)

    return modes
"""Persona loader - loads persona configuration from YAML files."""

from pathlib import Path
from typing import Optional

import yaml
from makima_common.logging import get_logger
from makima_schemas import Persona

logger = get_logger(__name__)

# Global current persona
_current_persona: Optional[Persona] = None


def load_persona_from_file(file_path: Path) -> Persona:
    """Load persona from a YAML file.

    Args:
        file_path: Path to the persona YAML file

    Returns:
        Persona instance

    Raises:
        FileNotFoundError: If file doesn't exist
        yaml.YAMLError: If YAML is invalid
    """
    logger.info("Loading persona from file", path=str(file_path))

    with open(file_path, "r", encoding="utf-8") as f:
        data = yaml.safe_load(f)

    if not data:
        logger.warning("Empty persona file, using defaults", path=str(file_path))
        return Persona()

    # Create Persona instance from YAML data
    persona = Persona(**data)
    logger.info("Loaded persona", name=persona.name, traits=persona.traits)

    return persona


def load_persona(project_root: Path) -> Persona:
    """Load persona from project's .makima/persona.yaml file.

    Args:
        project_root: Root directory of the project

    Returns:
        Persona instance (or default if file not found)
    """
    persona_file = project_root / ".makima" / "persona.yaml"

    if persona_file.exists():
        try:
            return load_persona_from_file(persona_file)
        except Exception as e:
            logger.error("Failed to load persona", error=str(e), path=str(persona_file))
            logger.info("Using default persona")
            from makima.persona.builtin import DEFAULT_PERSONA
            return DEFAULT_PERSONA
    else:
        logger.debug("No persona.yaml found, using default", path=str(persona_file))
        from makima.persona.builtin import DEFAULT_PERSONA
        return DEFAULT_PERSONA


def get_current_persona() -> Persona:
    """Get the current active persona.

    Returns:
        Current Persona instance
    """
    global _current_persona
    if _current_persona is None:
        # Load from project root
        import os
        project_root = Path(os.getcwd())
        _current_persona = load_persona(project_root)
    return _current_persona


def set_current_persona(persona: Persona) -> None:
    """Set the current active persona.

    Args:
        persona: New persona to set as current
    """
    global _current_persona
    _current_persona = persona
    logger.info("Persona updated", name=persona.name)
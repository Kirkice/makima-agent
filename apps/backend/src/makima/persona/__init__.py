"""Persona module - manages Agent's core personality."""

from makima.persona.loader import (
    load_persona_from_file,
    load_persona,
    get_current_persona,
    set_current_persona,
)
from makima.persona.builtin import DEFAULT_PERSONA

__all__ = [
    "load_persona_from_file",
    "load_persona",
    "get_current_persona",
    "set_current_persona",
    "DEFAULT_PERSONA",
]
"""Mode registry — manages all available modes."""

from __future__ import annotations

from makima.modes.builtin import BUILTIN_MODES
from makima_schemas import ModeConfig
from makima_common.logging import get_logger

logger = get_logger(__name__)

# Default mode slug
DEFAULT_MODE_SLUG = "code"

# Mode registry: slug -> ModeConfig
_MODE_REGISTRY: dict[str, ModeConfig] = {}


def _init_registry() -> None:
    """Initialize the mode registry with built-in modes."""
    global _MODE_REGISTRY
    if _MODE_REGISTRY:
        return  # Already initialized

    for mode in BUILTIN_MODES:
        _MODE_REGISTRY[mode.slug] = mode
        logger.debug("Registered built-in mode", slug=mode.slug, name=mode.name)


def register_mode(mode: ModeConfig) -> None:
    """Register a mode configuration.

    Args:
        mode: The mode configuration to register.
    """
    _init_registry()
    _MODE_REGISTRY[mode.slug] = mode
    logger.info("Registered mode", slug=mode.slug, name=mode.name, source=mode.source)


def unregister_mode(slug: str) -> ModeConfig | None:
    """Unregister a mode by slug.

    Args:
        slug: The mode slug to unregister.

    Returns:
        The removed ModeConfig, or None if not found.
    """
    _init_registry()
    mode = _MODE_REGISTRY.pop(slug, None)
    if mode:
        logger.info("Unregistered mode", slug=slug)
    return mode


def get_mode(slug: str) -> ModeConfig | None:
    """Get a mode configuration by slug.

    Args:
        slug: The mode slug identifier.

    Returns:
        The ModeConfig, or None if not found.
    """
    _init_registry()
    return _MODE_REGISTRY.get(slug)


def get_default_mode() -> ModeConfig:
    """Get the default mode configuration.

    Returns:
        The default ModeConfig (code mode).
    """
    _init_registry()
    mode = _MODE_REGISTRY.get(DEFAULT_MODE_SLUG)
    if mode is None:
        # Fallback to first available mode
        mode = next(iter(_MODE_REGISTRY.values()))
    return mode


def list_modes() -> list[ModeConfig]:
    """List all registered modes.

    Returns:
        List of all registered ModeConfig objects.
    """
    _init_registry()
    return list(_MODE_REGISTRY.values())


def has_mode(slug: str) -> bool:
    """Check if a mode is registered.

    Args:
        slug: The mode slug to check.

    Returns:
        True if the mode exists, False otherwise.
    """
    _init_registry()
    return slug in _MODE_REGISTRY
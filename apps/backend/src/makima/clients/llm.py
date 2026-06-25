"""LLM client wrapper."""

from __future__ import annotations

from functools import lru_cache

from langchain_openai import ChatOpenAI

from makima_common.config import get_settings
from makima_schemas import ModeConfig


@lru_cache
def get_chat_model() -> ChatOpenAI:
    """Return a configured ChatOpenAI instance with default settings."""
    settings = get_settings()
    return ChatOpenAI(
        model=settings.llm_model,
        temperature=settings.llm_temperature,
        max_tokens=settings.llm_max_tokens,
        api_key=settings.llm_api_key,
        base_url=settings.llm_api_base if settings.llm_api_base != "https://api.openai.com/v1" else None,
    )


def get_chat_model_with_temperature(temperature: float) -> ChatOpenAI:
    """Return a ChatOpenAI instance with a specific temperature.

    Args:
        temperature: The temperature to use for the LLM.

    Returns:
        A ChatOpenAI instance with the specified temperature.
    """
    settings = get_settings()
    return ChatOpenAI(
        model=settings.llm_model,
        temperature=temperature,
        max_tokens=settings.llm_max_tokens,
        api_key=settings.llm_api_key,
        base_url=settings.llm_api_base if settings.llm_api_base != "https://api.openai.com/v1" else None,
    )


def get_chat_model_for_mode(mode: ModeConfig) -> ChatOpenAI:
    """Return a ChatOpenAI instance configured for a specific mode.

    If the mode has specific LLM configuration (model, api_base, api_key),
    use those settings. Otherwise, fall back to global settings.

    Args:
        mode: The ModeConfig containing optional LLM overrides.

    Returns:
        A ChatOpenAI instance configured for the mode.
    """
    settings = get_settings()
    
    # Use mode-specific settings if available, otherwise use global settings
    model = mode.model if mode.model else settings.llm_model
    api_key = mode.api_key if mode.api_key else settings.llm_api_key
    api_base = mode.api_base if mode.api_base else settings.llm_api_base
    
    # Handle base_url logic (same as other functions)
    base_url = api_base if api_base != "https://api.openai.com/v1" else None
    
    return ChatOpenAI(
        model=model,
        temperature=mode.temperature,
        max_tokens=settings.llm_max_tokens,
        api_key=api_key,
        base_url=base_url,
    )


def get_chat_model_with_override(
    mode: ModeConfig,
    model_name: str | None = None,
    api_key: str | None = None,
    base_url: str | None = None,
    temperature: float | None = None,
) -> ChatOpenAI:
    """Return a ChatOpenAI with client-provided overrides applied on top of mode config.

    Priority: client override > mode config > global settings.

    Args:
        mode: The base ModeConfig.
        model_name: Client-provided model identifier override.
        api_key: Client-provided API key override.
        base_url: Client-provided API base URL override.
        temperature: Client-provided temperature override.

    Returns:
        A ChatOpenAI instance with the resolved configuration.
    """
    settings = get_settings()

    # Start with mode-level config, then apply client overrides
    resolved_model = model_name or (mode.model if mode.model else settings.llm_model)
    resolved_api_key = api_key or (mode.api_key if mode.api_key else settings.llm_api_key)
    resolved_api_base = base_url or (mode.api_base if mode.api_base else settings.llm_api_base)
    resolved_temp = temperature if temperature is not None else mode.temperature

    resolved_base_url = resolved_api_base if resolved_api_base != "https://api.openai.com/v1" else None

    return ChatOpenAI(
        model=resolved_model,
        temperature=resolved_temp,
        max_tokens=settings.llm_max_tokens,
        api_key=resolved_api_key,
        base_url=resolved_base_url,
    )

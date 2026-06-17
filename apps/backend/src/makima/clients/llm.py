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
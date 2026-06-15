"""LLM client wrapper."""

from __future__ import annotations

from functools import lru_cache

from langchain_openai import ChatOpenAI

from makima_common.config import get_settings


@lru_cache
def get_chat_model() -> ChatOpenAI:
    """Return a configured ChatOpenAI instance."""
    settings = get_settings()
    return ChatOpenAI(
        model=settings.llm_model,
        temperature=settings.llm_temperature,
        max_tokens=settings.llm_max_tokens,
        api_key=settings.llm_api_key,
        base_url=settings.llm_api_base if settings.llm_api_base != "https://api.openai.com/v1" else None,
    )
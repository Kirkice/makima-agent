"""Application configuration loaded from environment variables."""

from __future__ import annotations

from functools import lru_cache
from typing import Literal

from pydantic_settings import BaseSettings, SettingsConfigDict


class Settings(BaseSettings):
    """Root settings object.

    Reads values from environment variables (with ``MAKIMA_`` prefix) or
    ``.env`` files.  All values have sensible defaults for local development.
    """

    model_config = SettingsConfigDict(
        env_prefix="MAKIMA_",
        env_file=".env",
        env_file_encoding="utf-8",
        extra="ignore",
    )

    # ── General ──────────────────────────────────────────────────────
    app_name: str = "makima-agent"
    debug: bool = False
    environment: Literal["development", "staging", "production"] = "development"
    api_secret_key: str = "change-me-to-a-random-string"
    api_cors_origins: list[str] = ["http://localhost:3000", "http://localhost:8000"]

    # ── Database ─────────────────────────────────────────────────────
    database_url: str = "postgresql+asyncpg://makima:makima@localhost:5432/makima"

    # ── Redis ────────────────────────────────────────────────────────
    redis_url: str = "redis://localhost:6379/0"

    # ── LLM ──────────────────────────────────────────────────────────
    llm_api_key: str = ""
    llm_api_base: str = "https://api.openai.com/v1"
    llm_model: str = "gpt-4o"
    llm_temperature: float = 0.7
    llm_max_tokens: int = 4096

    # ── Orchestrator ─────────────────────────────────────────────────
    orchestrator_max_steps: int = 50
    orchestrator_timeout: int = 300

    # ── Tool Runtime ─────────────────────────────────────────────────
    tool_sandbox_enabled: bool = True
    tool_timeout: int = 60
    tool_working_dir: str = "/tmp/makima-sandbox"

    # ── Memory Service (Mem0) ──────────────────────────────────────────
    memory_enabled: bool = True
    mem0_embedding_model: str = "text-embedding-3-small"
    mem0_vector_store: str = "pgvector"

    # ── Knowledge Service (RAG) ────────────────────────────────────────
    knowledge_enabled: bool = True
    rag_chunk_size: int = 1000
    rag_chunk_overlap: int = 100
    rag_top_k: int = 5


@lru_cache
def get_settings() -> Settings:
    """Return a cached ``Settings`` instance."""
    return Settings()  # type: ignore[call-arg]
"""Types and models for the prompt engine."""

from __future__ import annotations

from typing import Any

from pydantic import BaseModel, Field

from makima_schemas import ModeConfig, Persona


class ToolDescription(BaseModel):
    """Description of a tool for prompt generation."""

    name: str = Field(..., description="Tool name")
    description: str = Field(..., description="Tool description")
    parameters: dict[str, Any] = Field(default_factory=dict, description="Parameter schema")
    risk_level: str = Field(default="low", description="Risk level: low, medium, high")
    requires_approval: bool = Field(default=False, description="Whether user approval is required")


class MemoryContext(BaseModel):
    """Memory context to inject into prompt."""

    user_preferences: list[str] = Field(default_factory=list, description="User preference facts")
    conversation_summary: str | None = Field(default=None, description="Summary of past conversations")
    relevant_facts: list[str] = Field(default_factory=list, description="Relevant facts from memory")


class KnowledgeContext(BaseModel):
    """Knowledge base context to inject into prompt."""

    relevant_documents: list[str] = Field(default_factory=list, description="Relevant document excerpts")
    search_results: list[str] = Field(default_factory=list, description="RAG search results")


class PromptContext(BaseModel):
    """Complete context for generating a system prompt."""

    mode: ModeConfig = Field(..., description="Current agent mode")
    persona: Persona | None = Field(default=None, description="Agent persona (personality, speaking style)")
    available_tools: list[ToolDescription] = Field(default_factory=list, description="Available tools")
    available_modes: list[ModeConfig] = Field(default_factory=list, description="All available modes")
    memory: MemoryContext | None = Field(default=None, description="Memory context")
    knowledge: KnowledgeContext | None = Field(default=None, description="Knowledge context")
    session_id: str | None = Field(default=None, description="Current session ID")
    user_id: str | None = Field(default=None, description="Current user ID")
    custom_variables: dict[str, str] = Field(default_factory=dict, description="Custom template variables")

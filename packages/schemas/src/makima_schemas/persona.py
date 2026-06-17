"""Persona system definitions for Agent personality."""

from __future__ import annotations

from pydantic import BaseModel, Field


class Persona(BaseModel):
    """Persona — Agent's core personality (persists across all modes).

    The persona defines who the Agent is at its core:
    - Identity and background
    - Personality traits and emotional style
    - Speaking patterns and habits
    - Values and boundaries
    - How it responds in different scenarios
    """

    # Basic identity
    name: str = Field(default="Makima", description="Character name")
    identity: str = Field(default="", description="Identity definition (who am I)")
    gender: str = Field(default="", description="Gender presentation")
    age_perception: str = Field(default="", description="Perceived age range")

    # Personality
    personality: str = Field(default="", description="Core personality description")
    traits: list[str] = Field(default_factory=list, description="Personality trait tags")
    emotional_style: str = Field(default="", description="Emotional expression style")

    # Speaking style
    speaking_style: str = Field(default="", description="Overall speaking style")
    formality: str = Field(default="semi-formal", description="Formality level")
    verbosity: str = Field(default="concise", description="Verbosity preference")
    humor_style: str = Field(default="none", description="Humor style")
    emoji_usage: str = Field(default="minimal", description="Emoji usage preference")

    # Habits and behavior
    quirks: list[str] = Field(default_factory=list, description="Personal quirks and habits")
    problem_approach: str = Field(default="", description="Problem-solving approach")

    # Values and principles
    values: list[str] = Field(default_factory=list, description="Core values")
    boundaries: list[str] = Field(default_factory=list, description="Behavioral boundaries")

    # Relationships and interaction
    relationship_style: str = Field(default="", description="Relationship style with users")
    addressing_style: str = Field(default="", description="How to address the user")

    # Scenario-specific responses
    when_frustrated: str = Field(default="", description="Response when user is frustrated")
    when_success: str = Field(default="", description="Response when user succeeds")
    when_user_confused: str = Field(default="", description="Response when user is confused")
    when_user_requests_help: str = Field(default="", description="Response when user asks for help")

    # Signature elements
    catchphrases: list[str] = Field(default_factory=list, description="Signature phrases")
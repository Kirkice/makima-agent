"""PromptEngine — assembles system prompts from modular sections."""

from __future__ import annotations

from makima_schemas import ModeConfig, Persona
from makima_common.logging import get_logger

from makima.prompts.sections import (
    build_capabilities_section,
    build_context_section,
    build_custom_section,
    build_emotion_section,
    build_modes_section,
    build_personality_section,
    build_role_section,
    build_rules_section,
    build_tools_section,
)
from makima.prompts.types import KnowledgeContext, MemoryContext, PromptContext

logger = get_logger(__name__)


class PromptEngine:
    """Engine for building system prompts from modular sections.

    The engine assembles a complete system prompt by combining:
    - Role definition (from mode config)
    - Capabilities (based on mode's tool groups)
    - Tool descriptions (detailed tool documentation)
    - Context injection (memory + knowledge)
    - Rules and guidelines
    - Available modes (for mode switching)
    - Custom instructions (mode-specific)
    """

    def __init__(self) -> None:
        """Initialize the prompt engine."""
        logger.debug("PromptEngine initialized")

    def build_system_prompt(self, ctx: PromptContext) -> str:
        """Build a complete system prompt from the given context.

        Args:
            ctx: The prompt context containing mode, tools, memory, and knowledge.

        Returns:
            The assembled system prompt string.
        """
        sections: list[str] = []

        # 1. Role definition (always first)
        role = build_role_section(ctx.mode)
        if role:
            sections.append(role)

        # 2. Personality (from persona, if available)
        if ctx.persona:
            personality = build_personality_section(ctx.persona)
            if personality:
                sections.append(personality)

        # 3. Capabilities
        capabilities = build_capabilities_section(ctx)
        if capabilities:
            sections.append(capabilities)

        # 3. Tool descriptions
        tools = build_tools_section(ctx.available_tools)
        if tools:
            sections.append(tools)

        # 4. Context injection (memory + knowledge)
        context = build_context_section(ctx)
        if context:
            sections.append(context)

        # 5. Rules and guidelines
        rules = build_rules_section()
        if rules:
            sections.append(rules)

        # 6. Available modes
        modes = build_modes_section(ctx.available_modes)
        if modes:
            sections.append(modes)

        # 7. Custom instructions
        custom = build_custom_section(ctx.mode)
        if custom:
            sections.append(custom)

        # 8. Emotion instructions (for avatar expression)
        emotion = build_emotion_section()
        if emotion:
            sections.append(emotion)

        # Join all sections
        prompt = "\n\n".join(sections)

        logger.debug(
            "Built system prompt",
            mode=ctx.mode.slug,
            num_tools=len(ctx.available_tools),
            num_sections=len(sections),
            prompt_length=len(prompt),
        )

        return prompt

    def build_system_prompt_simple(
        self,
        mode: ModeConfig,
        memory_context: str | None = None,
        knowledge_context: str | None = None,
        persona: Persona | None = None,
    ) -> str:
        """Build a simplified system prompt with just mode and optional contexts.

        This is a convenience method for backward compatibility with the existing
        orchestrator that uses simple string contexts.

        Args:
            mode: The current mode configuration.
            memory_context: Optional memory context string.
            knowledge_context: Optional knowledge context string.
            persona: Optional persona configuration.

        Returns:
            The assembled system prompt string.
        """
        # Build memory context
        memory = None
        if memory_context:
            memory = MemoryContext(
                relevant_facts=[memory_context] if memory_context else []
            )

        # Build knowledge context
        knowledge = None
        if knowledge_context:
            knowledge = KnowledgeContext(
                search_results=[knowledge_context] if knowledge_context else []
            )

        ctx = PromptContext(
            mode=mode,
            persona=persona,
            available_tools=[],
            available_modes=[],
            memory=memory,
            knowledge=knowledge,
        )

        return self.build_system_prompt(ctx)

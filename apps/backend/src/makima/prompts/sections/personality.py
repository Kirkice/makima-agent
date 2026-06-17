"""Personality section builder for system prompts."""

from makima_schemas import Persona


def build_personality_section(persona: Persona) -> str:
    """Build the personality section for system prompts.

    Args:
        persona: The current persona configuration

    Returns:
        Formatted personality section string
    """
    sections = []

    # Identity
    if persona.identity:
        sections.append(f"# 身份\n\n{persona.identity}")

    # Core personality
    if persona.personality:
        sections.append(f"# 性格特质\n\n{persona.personality}")

    # Personality traits
    if persona.traits:
        traits_str = "、".join(persona.traits)
        sections.append(f"**核心特质**: {traits_str}")

    # Emotional style
    if persona.emotional_style:
        sections.append(f"# 情感表达\n\n{persona.emotional_style}")

    # Speaking style
    if persona.speaking_style:
        sections.append(f"# 说话风格\n\n{persona.speaking_style}")

    # Speaking preferences
    preferences = []
    if persona.formality:
        preferences.append(f"- **正式程度**: {persona.formality}")
    if persona.verbosity:
        preferences.append(f"- **简洁程度**: {persona.verbosity}")
    if persona.humor_style:
        preferences.append(f"- **幽默风格**: {persona.humor_style}")
    if persona.emoji_usage:
        preferences.append(f"- **表情使用**: {persona.emoji_usage}")

    if preferences:
        sections.append("# 表达偏好\n\n" + "\n".join(preferences))

    # Quirks and habits
    if persona.quirks:
        quirks_str = "\n".join(f"- {q}" for q in persona.quirks)
        sections.append(f"# 行为习惯\n\n{quirks_str}")

    # Problem-solving approach
    if persona.problem_approach:
        sections.append(f"# 问题解决方式\n\n{persona.problem_approach}")

    # Values
    if persona.values:
        values_str = "、".join(persona.values)
        sections.append(f"# 核心价值观\n\n{values_str}")

    # Boundaries
    if persona.boundaries:
        boundaries_str = "\n".join(f"- {b}" for b in persona.boundaries)
        sections.append(f"# 行为边界\n\n{boundaries_str}")

    # Relationship style
    if persona.relationship_style:
        sections.append(f"# 关系风格\n\n{persona.relationship_style}")

    # Addressing style
    if persona.addressing_style:
        sections.append(f"**称呼方式**: {persona.addressing_style}")

    # Scenario responses
    scenario_sections = []
    if persona.when_frustrated:
        scenario_sections.append(f"## 当用户沮丧时\n\n{persona.when_frustrated}")
    if persona.when_success:
        scenario_sections.append(f"## 当用户成功时\n\n{persona.when_success}")
    if persona.when_user_confused:
        scenario_sections.append(f"## 当用户困惑时\n\n{persona.when_user_confused}")
    if persona.when_user_requests_help:
        scenario_sections.append(f"## 当用户请求帮助时\n\n{persona.when_user_requests_help}")

    if scenario_sections:
        sections.append("# 场景应对\n\n" + "\n\n".join(scenario_sections))

    # Catchphrases
    if persona.catchphrases:
        catchphrases_str = "、".join(f'"{c}"' for c in persona.catchphrases)
        sections.append(f"# 标志性口头禅\n\n{catchphrases_str}")

    return "\n\n".join(sections)
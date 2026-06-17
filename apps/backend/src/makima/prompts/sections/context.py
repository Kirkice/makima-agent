"""Context injection section for system prompt."""

from makima.prompts.types import PromptContext


def build_context_section(ctx: PromptContext) -> str:
    """Build the context section with memory and knowledge."""
    sections = []

    # Memory context
    if ctx.memory:
        memory_lines = []

        if ctx.memory.user_preferences:
            memory_lines.append("### User Preferences")
            for pref in ctx.memory.user_preferences:
                memory_lines.append(f"- {pref}")

        if ctx.memory.conversation_summary:
            memory_lines.append("\n### Previous Conversation Summary")
            memory_lines.append(ctx.memory.conversation_summary)

        if ctx.memory.relevant_facts:
            memory_lines.append("\n### Relevant Facts")
            for fact in ctx.memory.relevant_facts:
                memory_lines.append(f"- {fact}")

        if memory_lines:
            sections.append("# Memory Context\n\n" + "\n".join(memory_lines))

    # Knowledge context
    if ctx.knowledge:
        knowledge_lines = []

        if ctx.knowledge.relevant_documents:
            knowledge_lines.append("### Relevant Documents")
            for doc in ctx.knowledge.relevant_documents:
                knowledge_lines.append(f"- {doc}")

        if ctx.knowledge.search_results:
            knowledge_lines.append("\n### Knowledge Base Results")
            for result in ctx.knowledge.search_results:
                knowledge_lines.append(result)

        if knowledge_lines:
            sections.append("# Knowledge Context\n\n" + "\n".join(knowledge_lines))

    return "\n\n".join(sections) if sections else ""
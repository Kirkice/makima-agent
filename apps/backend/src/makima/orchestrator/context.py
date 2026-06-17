"""上下文窗口管理 — 追踪 token 使用、自动摘要、截断、超限检测。

借鉴 Zoo Code 的 context-tracking 设计：
- 实时追踪当前对话的 token 使用量
- 自动摘要旧消息以节省空间
- 工具结果超长时自动截断
- 超限错误检测 + 自动降级重试

与 Rust TokenCounterService 配合使用。
"""

from __future__ import annotations

import asyncio
from typing import Any

from langchain_core.messages import BaseMessage, HumanMessage, SystemMessage

from makima_common.logging import get_logger
from makima.clients.rust_client import get_rust_client

logger = get_logger(__name__)


class ContextWindowConfig:
    """上下文窗口配置。"""

    def __init__(
        self,
        model: str = "gpt-4",
        max_tokens: int = 128000,  # 模型最大 token 数
        reserved_for_response: int = 4096,  # 预留给响应的 token 数
        summary_threshold: float = 0.7,  # 达到 70% 时触发摘要
        tool_result_max_tokens: int = 4000,  # 工具结果最大 token 数
        use_rust_counter: bool = True,  # 是否使用 Rust token 计数器
    ) -> None:
        self.model = model
        self.max_tokens = max_tokens
        self.reserved_for_response = reserved_for_response
        self.summary_threshold = summary_threshold
        self.tool_result_max_tokens = tool_result_max_tokens
        self.use_rust_counter = use_rust_counter

    @property
    def available_tokens(self) -> int:
        """可用于 prompt 的 token 数。"""
        return self.max_tokens - self.reserved_for_response

    @property
    def summary_trigger_tokens(self) -> int:
        """触发摘要的 token 阈值。"""
        return int(self.available_tokens * self.summary_threshold)


class ContextWindowTracker:
    """上下文窗口追踪器。

    负责：
    1. 实时计算当前对话的 token 使用量
    2. 检测是否接近或超过限制
    3. 提供压缩策略（摘要、截断）
    """

    def __init__(self, config: ContextWindowConfig | None = None) -> None:
        self.config = config or ContextWindowConfig()
        self._current_tokens: int = 0
        self._rust_available: bool | None = None

    async def _check_rust_available(self) -> bool:
        """检查 Rust 服务是否可用。"""
        if self._rust_available is not None:
            return self._rust_available

        if not self.config.use_rust_counter:
            self._rust_available = False
            return False

        try:
            client = get_rust_client()
            self._rust_available = await client.is_available()
        except Exception:
            self._rust_available = False

        return self._rust_available

    async def count_tokens(self, text: str) -> int:
        """计算文本的 token 数量。

        优先使用 Rust 服务，如果不可用则使用简单估算。
        """
        if await self._check_rust_available():
            try:
                client = get_rust_client()
                result = await client.count_tokens(text, self.config.model)
                return result.get("token_count", 0)
            except Exception as e:
                logger.warning("Rust token count failed, using fallback", error=str(e))

        # 简单估算：1 token ≈ 4 字符（英文）或 2 字符（中文）
        # 使用混合估算
        chinese_chars = sum(1 for c in text if '\u4e00' <= c <= '\u9fff')
        other_chars = len(text) - chinese_chars
        estimated = (chinese_chars / 2) + (other_chars / 4)
        return int(estimated)

    async def count_messages_tokens(self, messages: list[BaseMessage]) -> int:
        """计算消息列表的总 token 数。

        包括消息内容和系统开销（每条消息约 4 tokens）。
        """
        total = 0
        for msg in messages:
            # 消息开销：role + content + 分隔符 ≈ 4 tokens
            content = msg.content if isinstance(msg.content, str) else str(msg.content)
            msg_tokens = await self.count_tokens(content) + 4
            total += msg_tokens
        return total

    async def update_token_count(self, messages: list[BaseMessage]) -> int:
        """更新当前 token 计数。"""
        self._current_tokens = await self.count_messages_tokens(messages)
        return self._current_tokens

    @property
    def current_tokens(self) -> int:
        """当前 token 使用量。"""
        return self._current_tokens

    @property
    def usage_ratio(self) -> float:
        """当前使用率。"""
        return self._current_tokens / self.config.available_tokens

    def is_near_limit(self) -> bool:
        """是否接近限制。"""
        return self._current_tokens >= self.config.summary_trigger_tokens

    def is_over_limit(self) -> bool:
        """是否超过限制。"""
        return self._current_tokens >= self.config.available_tokens

    async def truncate_tool_result(self, result: str) -> tuple[str, bool]:
        """截断工具结果如果过长。

        Args:
            result: 工具返回的结果字符串

        Returns:
            (truncated_result, was_truncated)
        """
        result_tokens = await self.count_tokens(result)

        if result_tokens <= self.config.tool_result_max_tokens:
            return result, False

        # 需要截断
        if await self._check_rust_available():
            try:
                client = get_rust_client()
                response = await client.truncate_tokens(
                    result,
                    self.config.tool_result_max_tokens,
                    self.config.model,
                    preserve_start=False,  # 保留末尾（通常包含最新结果）
                )
                truncated = response.get("truncated_text", result)
                return truncated, True
            except Exception as e:
                logger.warning("Rust truncate failed, using fallback", error=str(e))

        # 简单截断：保留末尾部分
        chars_per_token = 4  # 估算
        max_chars = self.config.tool_result_max_tokens * chars_per_token
        if len(result) > max_chars:
            truncated = "...[截断]...\n" + result[-max_chars:]
            return truncated, True

        return result, False

    async def summarize_messages(
        self,
        messages: list[BaseMessage],
        llm: Any,
        keep_recent: int = 5,
    ) -> list[BaseMessage]:
        """摘要旧消息以节省空间。

        Args:
            messages: 原始消息列表
            llm: LLM 实例用于生成摘要
            keep_recent: 保留最近的 N 条消息

        Returns:
            压缩后的消息列表
        """
        if len(messages) <= keep_recent + 1:
            return messages

        # 分离旧消息和最新消息
        old_messages = messages[:-keep_recent]
        recent_messages = messages[-keep_recent:]

        # 生成摘要
        summary_prompt = """请将以下对话历史压缩为简洁的摘要，保留关键信息：

对话历史：
{history}

请生成一段简短的摘要（不超过 200 字）："""

        history_text = "\n".join(
            f"{msg.type}: {msg.content if isinstance(msg.content, str) else str(msg.content)[:100]}"
            for msg in old_messages
        )

        try:
            summary_response = await llm.ainvoke([
                HumanMessage(content=summary_prompt.format(history=history_text))
            ])
            summary_text = summary_response.content
        except Exception as e:
            logger.warning("Failed to generate summary, using fallback", error=str(e))
            summary_text = f"[对话历史摘要：共 {len(old_messages)} 条消息]"

        # 构建压缩后的消息列表
        summary_message = SystemMessage(content=f"[之前的对话摘要]\n{summary_text}")
        compressed = [summary_message] + recent_messages

        logger.info(
            "Messages summarized",
            original_count=len(messages),
            compressed_count=len(compressed),
            old_count=len(old_messages),
        )

        return compressed

    async def compress_if_needed(
        self,
        messages: list[BaseMessage],
        llm: Any,
    ) -> tuple[list[BaseMessage], bool]:
        """如果上下文接近限制，自动压缩。

        Args:
            messages: 当前消息列表
            llm: LLM 实例

        Returns:
            (compressed_messages, was_compressed)
        """
        current_tokens = await self.update_token_count(messages)

        if not self.is_near_limit():
            return messages, False

        logger.info(
            "Context near limit, compressing",
            current_tokens=current_tokens,
            threshold=self.config.summary_trigger_tokens,
        )

        compressed = await self.summarize_messages(messages, llm)

        # 更新计数
        new_tokens = await self.update_token_count(compressed)
        logger.info(
            "Context compresseded",
            before_tokens=current_tokens,
            after_tokens=new_tokens,
            saved_tokens=current_tokens - new_tokens,
        )

        return compressed, True


class ContextManager:
    """上下文管理器 — 整合追踪、压缩、截断功能。

    在 orchestrator 中使用，管理整个对话的上下文生命周期。
    """

    def __init__(self, config: ContextWindowConfig | None = None) -> None:
        self.config = config or ContextWindowConfig()
        self.tracker = ContextWindowTracker(self.config)
        self._compression_count: int = 0

    async def prepare_messages(
        self,
        messages: list[BaseMessage],
        llm: Any,
        system_prompt: str | None = None,
    ) -> list[BaseMessage]:
        """准备消息列表用于 LLM 调用。

        1. 添加系统提示（如果有）
        2. 压缩旧消息（如果需要）
        3. 更新 token 计数

        Args:
            messages: 对话消息列表
            llm: LLM 实例
            system_prompt: 系统提示

        Returns:
            准备好的消息列表
        """
        prepared = list(messages)

        # 添加系统提示
        if system_prompt:
            system_msg = SystemMessage(content=system_prompt)
            prepared = [system_msg] + prepared

        # 压缩旧消息
        prepared, was_compressed = await self.tracker.compress_if_needed(prepared, llm)
        if was_compressed:
            self._compression_count += 1

        return prepared

    async def process_tool_result(self, result: str) -> tuple[str, bool]:
        """处理工具结果，如果过长则截断。"""
        return await self.tracker.truncate_tool_result(result)

    def get_stats(self) -> dict[str, Any]:
        """获取上下文统计信息。"""
        return {
            "current_tokens": self.tracker.current_tokens,
            "max_tokens": self.config.max_tokens,
            "available_tokens": self.config.available_tokens,
            "usage_ratio": self.tracker.usage_ratio,
            "is_near_limit": self.tracker.is_near_limit(),
            "is_over_limit": self.tracker.is_over_limit(),
            "compression_count": self._compression_count,
        }


# 全局实例
_context_manager: ContextManager | None = None


def get_context_manager() -> ContextManager:
    """获取全局上下文管理器实例。"""
    global _context_manager
    if _context_manager is None:
        _context_manager = ContextManager()
    return _context_manager
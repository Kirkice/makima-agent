"""API 重试机制 — 指数退避 + 流式中断重试 + 上下文窗口检测。

借鉴 Zoo Code 的 backoffAndAnnounce 设计，实现：
- 指数退避：base * 2^retry，上限 600s
- 429 错误解析 Retry-After header
- 流式中断重试（mid-stream failure）
- 上下文窗口超限检测
"""

from __future__ import annotations

import asyncio
import math
import re
import time
from collections.abc import AsyncIterator, Callable, Awaitable
from typing import Any, TypeVar

from makima_common.logging import get_logger

logger = get_logger(__name__)

T = TypeVar("T")

# 常量
MAX_EXPONENTIAL_BACKOFF_SECONDS = 600  # 10 分钟上限
DEFAULT_BASE_DELAY = 5  # 默认基础延迟 5 秒
MAX_RETRY_ATTEMPTS = 5  # 最大重试次数

# 上下文窗口超限错误模式
CONTEXT_WINDOW_PATTERNS = [
    # OpenAI
    r"maximum context length is \d+ tokens",
    r"reduce your prompt",
    r"This model's maximum context length is \d+ tokens",
    # Anthropic
    r"prompt is too long",
    r"max_tokens: \d+ > \d+",
    # OpenRouter
    r"context_length_exceeded",
    r"maximum context length",
    # 通用
    r"context.?window.?exceeded",
    r"token.?limit.?exceeded",
]


class RetryConfig:
    """重试配置。"""

    def __init__(
        self,
        base_delay: float = DEFAULT_BASE_DELAY,
        max_delay: float = MAX_EXPONENTIAL_BACKOFF_SECONDS,
        max_attempts: int = MAX_RETRY_ATTEMPTS,
        retry_on_context_overflow: bool = True,
        retry_on_rate_limit: bool = True,
        on_retry: Callable[[int, float, str], Awaitable[None]] | None = None,
    ) -> None:
        self.base_delay = base_delay
        self.max_delay = max_delay
        self.max_attempts = max_attempts
        self.retry_on_context_overflow = retry_on_context_overflow
        self.retry_on_rate_limit = retry_on_rate_limit
        self.on_retry = on_retry


class RetryContext:
    """重试上下文，记录重试状态。"""

    def __init__(self) -> None:
        self.attempt = 0
        self.total_delay = 0.0
        self.last_error: str | None = None
        self.started_at: float = time.time()

    @property
    def elapsed(self) -> float:
        return time.time() - self.started_at


def is_context_window_error(error: Exception) -> bool:
    """检测是否为上下文窗口超限错误。"""
    error_msg = str(error).lower()
    for pattern in CONTEXT_WINDOW_PATTERNS:
        if re.search(pattern, error_msg, re.IGNORECASE):
            return True
    return False


def is_rate_limit_error(error: Exception) -> bool:
    """检测是否为速率限制错误 (429)。"""
    error_msg = str(error).lower()
    return (
        "429" in error_msg
        or "rate limit" in error_msg
        or "too many requests" in error_msg
        or "quota exceeded" in error_msg
    )


def parse_retry_after(error: Exception) -> float | None:
    """从错误信息中解析 Retry-After 延迟。"""
    error_msg = str(error)

    # 尝试匹配 "Retry-After: N" 或 "retry after N seconds"
    match = re.search(r"(?:retry[- ]?(?:after)?[:\s]+)(\d+)", error_msg, re.IGNORECASE)
    if match:
        return float(match.group(1))

    # 尝试匹配 "try again in N seconds"
    match = re.search(r"try again in (\d+) seconds?", error_msg, re.IGNORECASE)
    if match:
        return float(match.group(1))

    return None


def calculate_backoff(attempt: int, config: RetryConfig, error: Exception) -> float:
    """计算退避延迟。"""
    # 指数退避
    exponential_delay = min(
        math.ceil(config.base_delay * (2 ** attempt)),
        config.max_delay,
    )

    # 检查是否有服务端建议的延迟
    retry_after = parse_retry_after(error)
    if retry_after is not None:
        return max(exponential_delay, retry_after)

    return exponential_delay


async def with_retry(
    func: Callable[..., Awaitable[T]],
    *args: Any,
    config: RetryConfig | None = None,
    **kwargs: Any,
) -> T:
    """包装一个异步函数，添加重试逻辑。

    Args:
        func: 要执行的异步函数
        *args: 函数参数
        config: 重试配置
        **kwargs: 函数关键字参数

    Returns:
        函数返回值

    Raises:
        最后一次失败的异常
    """
    if config is None:
        config = RetryConfig()

    ctx = RetryContext()
    last_exception: Exception | None = None

    for attempt in range(config.max_attempts):
        ctx.attempt = attempt
        try:
            result = await func(*args, **kwargs)
            if attempt > 0:
                logger.info(
                    "Retry succeeded",
                    attempt=attempt,
                    total_delay=ctx.total_delay,
                )
            return result

        except Exception as e:
            last_exception = e
            error_msg = str(e)

            # 检查是否是不可重试的错误
            if is_context_window_error(e):
                if not config.retry_on_context_overflow:
                    logger.error("Context window exceeded, not retrying", error=error_msg)
                    raise
                if attempt >= 2:  # 上下文错误最多重试 3 次
                    logger.error("Context window error, max retries reached", attempt=attempt)
                    raise
                logger.warning("Context window exceeded, will retry", attempt=attempt)

            elif is_rate_limit_error(e):
                if not config.retry_on_rate_limit:
                    logger.error("Rate limited, not retrying", error=error_msg)
                    raise
                logger.warning("Rate limited", attempt=attempt)

            else:
                # 其他错误，检查是否还有重试机会
                if attempt >= config.max_attempts - 1:
                    logger.error("Max retry attempts reached", attempt=attempt, error=error_msg)
                    raise

            # 计算延迟
            delay = calculate_backoff(attempt, config, e)
            ctx.total_delay += delay
            ctx.last_error = error_msg

            logger.warning(
                "Retrying after delay",
                attempt=attempt + 1,
                max_attempts=config.max_attempts,
                delay=delay,
                error=error_msg[:200],
            )

            # 通知回调
            if config.on_retry:
                await config.on_retry(attempt + 1, delay, error_msg)

            # 等待
            await asyncio.sleep(delay)

    # 不应该到达这里
    if last_exception:
        raise last_exception
    raise RuntimeError("Retry exhausted without exception")


async def retry_stream(
    stream_factory: Callable[..., AsyncIterator[T]],
    *args: Any,
    config: RetryConfig | None = None,
    **kwargs: Any,
) -> AsyncIterator[T]:
    """包装一个异步流，添加重试逻辑。

    当流在传输过程中失败时，会从头重新创建流。

    Args:
        stream_factory: 返回异步迭代器的工厂函数
        *args: 工厂函数参数
        config: 重试配置
        **kwargs: 工厂函数关键字参数

    Yields:
        流中的元素
    """
    if config is None:
        config = RetryConfig()

    ctx = RetryContext()

    for attempt in range(config.max_attempts):
        ctx.attempt = attempt
        try:
            stream = await stream_factory(*args, **kwargs)
            async for item in stream:
                yield item
            return  # 流正常结束

        except Exception as e:
            error_msg = str(e)

            if is_context_window_error(e):
                logger.error("Context window exceeded in stream", error=error_msg)
                raise

            if attempt >= config.max_attempts - 1:
                logger.error("Stream retry exhausted", attempt=attempt, error=error_msg)
                raise

            delay = calculate_backoff(attempt, config, e)
            ctx.total_delay += delay
            ctx.last_error = error_msg

            logger.warning(
                "Stream failed, retrying",
                attempt=attempt + 1,
                delay=delay,
                error=error_msg[:200],
            )

            if config.on_retry:
                await config.on_retry(attempt + 1, delay, error_msg)

            await asyncio.sleep(delay)


class RetryMiddleware:
    """重试中间件，可以包装 LLM 客户端调用。

    用法：
        middleware = RetryMiddleware(config)
        response = await middleware.execute(llm_client.agenerate, messages)
    """

    def __init__(self, config: RetryConfig | None = None) -> None:
        self.config = config or RetryConfig()

    async def execute(self, func: Callable[..., Awaitable[T]], *args: Any, **kwargs: Any) -> T:
        """执行函数并自动重试。"""
        return await with_retry(func, *args, config=self.config, **kwargs)
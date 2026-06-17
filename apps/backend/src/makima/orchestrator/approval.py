"""工具审批流程 — 基于风险等级的自动/手动审批状态机。

借鉴 Zoo Code 的 autoApprove 设计：
- low 风险：自动执行
- medium 风险：可配置（auto_approve 字段控制）
- high 风险：始终需要用户确认

状态机：
    pending → approved (auto) → executing
    pending → awaiting_approval → approved (user) → executing
    pending → awaiting_approval → rejected
"""

from __future__ import annotations

import asyncio
import time
import uuid
from enum import Enum
from typing import Any

from pydantic import BaseModel, Field

from makima_common.logging import get_logger

logger = get_logger(__name__)


class ApprovalStatus(str, Enum):
    """审批状态。"""
    PENDING = "pending"           # 等待审批决策
    AUTO_APPROVED = "auto_approved"  # 自动审批通过
    AWAITING_USER = "awaiting_user"  # 等待用户确认
    APPROVED = "approved"         # 用户审批通过
    REJECTED = "rejected"         # 用户拒绝
    TIMEOUT = "timeout"           # 超时


class ApprovalLevel(str, Enum):
    """审批级别。"""
    AUTO = "auto"         # 自动执行，无需审批
    NOTIFY = "notify"     # 通知用户但自动执行
    CONFIRM = "confirm"   # 需要用户确认
    DENY = "deny"         # 拒绝执行


class ApprovalRequest(BaseModel):
    """审批请求。"""
    id: str = Field(default_factory=lambda: str(uuid.uuid4())[:8])
    tool_name: str
    tool_args: dict[str, Any] = Field(default_factory=dict)
    risk_level: str = "low"  # low, medium, high
    session_id: str | None = None
    created_at: float = Field(default_factory=time.time)
    status: ApprovalStatus = ApprovalStatus.PENDING
    approved_by: str | None = None  # "auto" or "user"
    reason: str | None = None


class ApprovalResponse(BaseModel):
    """审批响应。"""
    request_id: str
    approved: bool
    reason: str | None = None


class ApprovalManager:
    """审批管理器。

    负责：
    1. 根据工具风险等级决定审批级别
    2. 管理审批请求的生命周期
    3. 支持异步等待用户响应
    """

    def __init__(
        self,
        auto_approve_low: bool = True,
        auto_approve_medium: bool = False,
        approval_timeout: float = 300.0,  # 5 分钟超时
    ) -> None:
        self.auto_approve_low = auto_approve_low
        self.auto_approve_medium = auto_approve_medium
        self.approval_timeout = approval_timeout
        self._pending_requests: dict[str, ApprovalRequest] = {}
        self._approval_events: dict[str, asyncio.Event] = {}
        self._approval_results: dict[str, ApprovalResponse] = {}

    def determine_approval_level(
        self,
        risk_level: str,
        auto_approve_override: bool | None = None,
    ) -> ApprovalLevel:
        """根据风险等级决定审批级别。

        Args:
            risk_level: 工具风险等级 ("low", "medium", "high")
            auto_approve_override: 覆盖默认审批行为（来自 ModeConfig.tool_groups）

        Returns:
            ApprovalLevel
        """
        # 如果有显式覆盖，使用覆盖值
        if auto_approve_override is not None:
            if auto_approve_override:
                return ApprovalLevel.AUTO
            else:
                return ApprovalLevel.CONFIRM

        # 根据风险等级决定
        if risk_level == "low":
            if self.auto_approve_low:
                return ApprovalLevel.AUTO
            return ApprovalLevel.NOTIFY

        elif risk_level == "medium":
            if self.auto_approve_medium:
                return ApprovalLevel.NOTIFY
            return ApprovalLevel.CONFIRM

        elif risk_level == "high":
            return ApprovalLevel.CONFIRM

        else:
            # 未知风险等级，默认需要确认
            logger.warning("Unknown risk level, requiring confirmation", risk_level=risk_level)
            return ApprovalLevel.CONFIRM

    async def request_approval(
        self,
        tool_name: str,
        tool_args: dict[str, Any],
        risk_level: str = "low",
        session_id: str | None = None,
        auto_approve_override: bool | None = None,
    ) -> ApprovalRequest:
        """创建审批请求并等待决策。

        Args:
            tool_name: 工具名称
            tool_args: 工具参数
            risk_level: 风险等级
            session_id: 会话 ID
            auto_approve_override: 覆盖默认审批行为

        Returns:
            ApprovalRequest（包含最终状态）
        """
        request = ApprovalRequest(
            tool_name=tool_name,
            tool_args=tool_args,
            risk_level=risk_level,
            session_id=session_id,
        )

        level = self.determine_approval_level(risk_level, auto_approve_override)

        if level == ApprovalLevel.AUTO:
            # 自动审批
            request.status = ApprovalStatus.AUTO_APPROVED
            request.approved_by = "auto"
            request.reason = "Auto-approved based on risk level"
            logger.debug(
                "Tool auto-approved",
                tool=tool_name,
                risk_level=risk_level,
            )
            return request

        elif level == ApprovalLevel.NOTIFY:
            # 通知用户但自动执行
            request.status = ApprovalStatus.AUTO_APPROVED
            request.approved_by = "auto"
            request.reason = "Auto-approved with notification"
            logger.info(
                "Tool auto-approved with notification",
                tool=tool_name,
                risk_level=risk_level,
                request_id=request.id,
            )
            # TODO: 发送通知事件给前端
            return request

        elif level == ApprovalLevel.CONFIRM:
            # 需要用户确认
            request.status = ApprovalStatus.AWAITING_USER
            logger.info(
                "Tool awaiting user approval",
                tool=tool_name,
                risk_level=risk_level,
                request_id=request.id,
            )

            # 存储请求并等待用户响应
            self._pending_requests[request.id] = request
            self._approval_events[request.id] = asyncio.Event()

            # TODO: 发送审批请求事件给前端（通过 SSE）
            # yield AgentEvent(type=APPROVAL_REQUESTED, data={...})

            try:
                # 等待用户响应或超时
                await asyncio.wait_for(
                    self._approval_events[request.id].wait(),
                    timeout=self.approval_timeout,
                )

                # 获取响应
                response = self._approval_results.get(request.id)
                if response and response.approved:
                    request.status = ApprovalStatus.APPROVED
                    request.approved_by = "user"
                    request.reason = response.reason
                else:
                    request.status = ApprovalStatus.REJECTED
                    request.reason = response.reason if response else "No response received"

            except asyncio.TimeoutError:
                request.status = ApprovalStatus.TIMEOUT
                request.reason = f"Approval timeout ({self.approval_timeout}s)"
                logger.warning(
                    "Approval request timed out",
                    request_id=request.id,
                    tool=tool_name,
                )

            finally:
                # 清理
                self._pending_requests.pop(request.id, None)
                self._approval_events.pop(request.id, None)
                self._approval_results.pop(request.id, None)

            return request

        else:  # DENY
            request.status = ApprovalStatus.REJECTED
            request.reason = "Tool execution denied by policy"
            return request

    async def respond_to_approval(
        self,
        request_id: str,
        approved: bool,
        reason: str | None = None,
    ) -> bool:
        """响应用户的审批决策。

        Args:
            request_id: 审批请求 ID
            approved: 是否批准
            reason: 原因

        Returns:
            True if the request was found and updated
        """
        if request_id not in self._pending_requests:
            logger.warning("Approval request not found", request_id=request_id)
            return False

        response = ApprovalResponse(
            request_id=request_id,
            approved=approved,
            reason=reason,
        )

        self._approval_results[request_id] = response

        # 唤醒等待的协程
        event = self._approval_events.get(request_id)
        if event:
            event.set()

        logger.info(
            "Approval response received",
            request_id=request_id,
            approved=approved,
        )

        return True

    def get_pending_requests(self) -> list[ApprovalRequest]:
        """获取所有待审批的请求。"""
        return list(self._pending_requests.values())

    def is_approved(self, request: ApprovalRequest) -> bool:
        """检查请求是否被批准。"""
        return request.status in (
            ApprovalStatus.AUTO_APPROVED,
            ApprovalStatus.APPROVED,
        )


# 全局实例
_approval_manager: ApprovalManager | None = None


def get_approval_manager() -> ApprovalManager:
    """获取全局审批管理器实例。"""
    global _approval_manager
    if _approval_manager is None:
        _approval_manager = ApprovalManager()
    return _approval_manager
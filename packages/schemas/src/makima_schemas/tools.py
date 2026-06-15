"""Tool protocol definitions for Phase 2."""

from __future__ import annotations

from typing import Any, Literal

from pydantic import BaseModel, Field


class ToolParameter(BaseModel):
    """Definition of a single tool parameter."""

    name: str = Field(..., description="参数名")
    type: str = Field(..., description="参数类型 (string, int, bool, list, dict)")
    description: str = Field(default="", description="参数描述")
    required: bool = Field(default=True, description="是否必填")


class ToolDefinition(BaseModel):
    """Complete definition of a tool."""

    name: str = Field(..., description="工具名称")
    description: str = Field(..., description="工具描述")
    parameters: list[ToolParameter] = Field(default_factory=list, description="参数列表")
    risk_level: Literal["low", "medium", "high"] = Field(default="low", description="风险等级")


class ToolCallRequest(BaseModel):
    """Request to invoke a tool."""

    tool_name: str = Field(..., description="工具名称")
    arguments: dict[str, Any] = Field(default_factory=dict, description="调用参数")


class ToolCallResult(BaseModel):
    """Result returned after tool execution."""

    tool_name: str = Field(..., description="工具名称")
    success: bool = Field(..., description="是否成功")
    output: Any = Field(default=None, description="工具输出")
    error: str | None = Field(default=None, description="错误信息")
    duration_ms: float = Field(default=0.0, description="执行耗时（毫秒）")
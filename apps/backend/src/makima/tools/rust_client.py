"""Rust Tool Runtime gRPC 客户端。

通过 gRPC 调用 Rust 实现的高性能工具执行服务。
如果 Rust 服务不可用，会自动降级到 Python 实现。
"""

from __future__ import annotations

import asyncio
from typing import Any

from makima_common.logging import get_logger

logger = get_logger(__name__)

# 尝试导入 grpc，如果不可用则标记为不可用
try:
    import grpc
    import grpc.aio
    GRPC_AVAILABLE = True
except ImportError:
    GRPC_AVAILABLE = False
    logger.warning("grpcio 未安装，Rust tool runtime 不可用，将使用 Python 实现")


class RustToolClient:
    """Rust tool runtime gRPC 客户端。
    
    通过 gRPC 与 Rust 服务通信，提供高性能的工具执行能力：
    - Shell 命令执行
    - 文件操作
    - HTTP 请求
    - 文档分块处理
    - 安全沙箱检查
    """

    def __init__(self, host: str = "localhost", port: int = 50051) -> None:
        self.address = f"{host}:{port}"
        self._channel: Any = None
        self._available: bool | None = None

    async def _get_channel(self) -> Any:
        """获取或创建 gRPC channel。"""
        if not GRPC_AVAILABLE:
            return None
        if self._channel is None:
            self._channel = grpc.aio.insecure_channel(self.address)
        return self._channel

    async def is_available(self) -> bool:
        """检查 Rust 服务是否可用。"""
        if not GRPC_AVAILABLE:
            return False
        if self._available is not None:
            return self._available
        try:
            channel = await self._get_channel()
            # 简单的健康检查：尝试发送一个检查命令
            state = channel.get_state(try_to_connect=True)
            await asyncio.sleep(0.1)
            state = channel.get_state()
            self._available = str(state) in ("ChannelConnectivity.READY", "ChannelConnectivity.IDLE")
        except Exception as e:
            logger.warning("Rust tool runtime 不可用", error=str(e))
            self._available = False
        return self._available or False

    async def close(self) -> None:
        """关闭 gRPC channel。"""
        if self._channel:
            await self._channel.close()
            self._channel = None

    # ── Shell 执行 ────────────────────────────────────────

    async def execute_shell(
        self,
        command: str,
        working_dir: str,
        timeout_seconds: int = 30,
    ) -> dict:
        """通过 Rust 执行 Shell 命令。
        
        Returns:
            dict with keys: success, stdout, stderr, exit_code, blocked, block_reason
        """
        if not await self.is_available():
            raise RuntimeError("Rust tool runtime 不可用")

        try:
            from makima.tools.proto import tool_runtime_pb2, tool_runtime_pb2_grpc
            channel = await self._get_channel()
            stub = tool_runtime_pb2_grpc.ShellServiceStub(channel)
            request = tool_runtime_pb2.ShellRequest(
                command=command,
                working_dir=working_dir,
                timeout_seconds=timeout_seconds,
            )
            response = await stub.Execute(request)
            return {
                "success": response.success,
                "stdout": response.stdout,
                "stderr": response.stderr,
                "exit_code": response.exit_code,
                "blocked": response.blocked,
                "block_reason": response.block_reason,
            }
        except Exception as e:
            logger.error("Rust shell execution failed", error=str(e))
            raise

    # ── 文件操作 ──────────────────────────────────────────

    async def read_file(self, path: str, base_dir: str) -> dict:
        """通过 Rust 读取文件。"""
        if not await self.is_available():
            raise RuntimeError("Rust tool runtime 不可用")

        try:
            from makima.tools.proto import tool_runtime_pb2, tool_runtime_pb2_grpc
            channel = await self._get_channel()
            stub = tool_runtime_pb2_grpc.FileServiceStub(channel)
            request = tool_runtime_pb2.ReadFileRequest(path=path, base_dir=base_dir)
            response = await stub.ReadFile(request)
            return {"success": response.success, "content": response.content, "error": response.error}
        except Exception as e:
            logger.error("Rust file read failed", error=str(e))
            raise

    async def write_file(self, path: str, content: str, base_dir: str) -> dict:
        """通过 Rust 写入文件。"""
        if not await self.is_available():
            raise RuntimeError("Rust tool runtime 不可用")

        try:
            from makima.tools.proto import tool_runtime_pb2, tool_runtime_pb2_grpc
            channel = await self._get_channel()
            stub = tool_runtime_pb2_grpc.FileServiceStub(channel)
            request = tool_runtime_pb2.WriteFileRequest(path=path, content=content, base_dir=base_dir)
            response = await stub.WriteFile(request)
            return {"success": response.success, "bytes_written": response.bytes_written, "error": response.error}
        except Exception as e:
            logger.error("Rust file write failed", error=str(e))
            raise

    async def list_directory(self, path: str, base_dir: str) -> dict:
        """通过 Rust 列出目录。"""
        if not await self.is_available():
            raise RuntimeError("Rust tool runtime 不可用")

        try:
            from makima.tools.proto import tool_runtime_pb2, tool_runtime_pb2_grpc
            channel = await self._get_channel()
            stub = tool_runtime_pb2_grpc.FileServiceStub(channel)
            request = tool_runtime_pb2.ListDirRequest(path=path, base_dir=base_dir)
            response = await stub.ListDirectory(request)
            return {
                "success": response.success,
                "entries": [{"name": e.name, "is_dir": e.is_dir, "size": e.size} for e in response.entries],
                "error": response.error,
            }
        except Exception as e:
            logger.error("Rust list directory failed", error=str(e))
            raise

    # ── HTTP 请求 ──────────────────────────────────────────

    async def http_request(
        self,
        url: str,
        method: str = "GET",
        headers: dict | None = None,
        body: str = "",
        timeout_seconds: int = 30,
    ) -> dict:
        """通过 Rust 发送 HTTP 请求。"""
        if not await self.is_available():
            raise RuntimeError("Rust tool runtime 不可用")

        try:
            from makima.tools.proto import tool_runtime_pb2, tool_runtime_pb2_grpc
            channel = await self._get_channel()
            stub = tool_runtime_pb2_grpc.HttpServiceStub(channel)
            request = tool_runtime_pb2.HttpRequest(
                url=url,
                method=method,
                headers=headers or {},
                body=body,
                timeout_seconds=timeout_seconds,
            )
            response = await stub.Request(request)
            return {
                "success": response.success,
                "status_code": response.status_code,
                "body": response.body,
                "blocked": response.blocked,
                "block_reason": response.block_reason,
            }
        except Exception as e:
            logger.error("Rust HTTP request failed", error=str(e))
            raise

    # ── 文档处理 ──────────────────────────────────────────

    async def chunk_text(self, text: str, chunk_size: int = 512, overlap: int = 50) -> dict:
        """通过 Rust 对文本进行分块处理。"""
        if not await self.is_available():
            raise RuntimeError("Rust tool runtime 不可用")

        try:
            from makima.tools.proto import tool_runtime_pb2, tool_runtime_pb2_grpc
            channel = await self._get_channel()
            stub = tool_runtime_pb2_grpc.DocumentServiceStub(channel)
            request = tool_runtime_pb2.ChunkTextRequest(
                text=text,
                chunk_size=chunk_size,
                overlap=overlap,
            )
            response = await stub.ChunkText(request)
            return {
                "chunks": [
                    {"index": c.index, "content": c.content, "token_count": c.token_count}
                    for c in response.chunks
                ],
                "total_chunks": response.total_chunks,
            }
        except Exception as e:
            logger.error("Rust chunk text failed", error=str(e))
            raise

    # ── 安全检查 ──────────────────────────────────────────

    async def check_path(self, path: str, base_dir: str) -> dict:
        """通过 Rust 检查路径安全性。"""
        if not await self.is_available():
            raise RuntimeError("Rust tool runtime 不可用")

        try:
            from makima.tools.proto import tool_runtime_pb2, tool_runtime_pb2_grpc
            channel = await self._get_channel()
            stub = tool_runtime_pb2_grpc.SandboxServiceStub(channel)
            request = tool_runtime_pb2.PathCheckRequest(path=path, base_dir=base_dir)
            response = await stub.CheckPath(request)
            return {"allowed": response.allowed, "resolved_path": response.resolved_path, "reason": response.reason}
        except Exception as e:
            logger.error("Rust path check failed", error=str(e))
            raise

    async def check_command(self, command: str) -> dict:
        """通过 Rust 检查命令安全性。"""
        if not await self.is_available():
            raise RuntimeError("Rust tool runtime 不可用")

        try:
            from makima.tools.proto import tool_runtime_pb2, tool_runtime_pb2_grpc
            channel = await self._get_channel()
            stub = tool_runtime_pb2_grpc.SandboxServiceStub(channel)
            request = tool_runtime_pb2.CommandCheckRequest(command=command)
            response = await stub.CheckCommand(request)
            return {"allowed": response.allowed, "matched_pattern": response.matched_pattern}
        except Exception as e:
            logger.error("Rust command check failed", error=str(e))
            raise


# 全局实例（懒加载）
_rust_client: RustToolClient | None = None


def get_rust_client() -> RustToolClient:
    """获取全局 Rust 工具客户端实例。"""
    global _rust_client
    if _rust_client is None:
        from makima_common.config import get_settings
        settings = get_settings()
        host = getattr(settings, "rust_tool_runtime_host", "localhost")
        port = getattr(settings, "rust_tool_runtime_port", 50051)
        _rust_client = RustToolClient(host=host, port=port)
    return _rust_client
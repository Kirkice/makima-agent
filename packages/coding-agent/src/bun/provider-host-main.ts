#!/usr/bin/env node
/**
 * Bun archive 内部的 Provider Host 子进程入口。
 *
 * Rust sidecar 不能把 Bun 编译后的 `pi` 当作 Node 解释器来执行 JavaScript entry；否则会递归
 * 回到 CLI selector。该受控标记仅用于 Provider Host child process，并在加载产品 CLI 之前
 * 直接启动内嵌 Host。
 */
import { startProviderHost as start } from "@earendil-works/pi-provider-host/runtime";

/** 启动 Bun 编译二进制中嵌入的 Provider Host。 */
export function startProviderHost(): void {
	start();
}

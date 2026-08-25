#!/usr/bin/env node
/**
 * 产品入口只负责在导入 TypeScript runtime 前选择运行时。
 *
 * native 被选中时，本文件不能静态导入 `main.ts`、Provider SDK 或 TUI；否则 sidecar 尚未启动时
 * 就已经创建了 TypeScript 产品路径，既破坏启动开销边界，也使 auto fallback 无法精确定义。
 */
import { fileURLToPath } from "node:url";
import {
	consumeRuntimeSelection,
	NATIVE_BOOTSTRAP_EXIT_CODE,
	resolveNativeRuntimeResources,
	resolvePackageDir,
	resolveRuntimeSelection,
	runNativeRuntime,
	runTypeScriptRuntime,
	runtimeFallbackRecord,
} from "./cli/runtime-selector.ts";

async function start(): Promise<void> {
	const rawArgs = process.argv.slice(2);
	if (rawArgs[0] === "--provider-host-child" && process.versions.bun) {
		// Bun archive 没有独立 Node 可执行文件。Rust 已在启动前校验紧随其后的 manifest entry，
		// 此处只将同一个 Bun 二进制切换为内嵌 Host，而不重新进入 runtime selector 形成递归。
		const { startProviderHost } = await import("./bun/provider-host-main.ts");
		startProviderHost();
		return;
	}

	const consumed = consumeRuntimeSelection(rawArgs);
	if (consumed.diagnostic) {
		process.stderr.write(`${consumed.diagnostic}\n`);
		process.exitCode = 2;
		return;
	}

	const resolved = resolveRuntimeSelection(consumed.selection, process.env.PI_RUNTIME);
	if (resolved.diagnostic) process.stderr.write(`${resolved.diagnostic}\n`);

	if (resolved.selection !== "ts") {
		try {
			const isBunBinary =
				import.meta.url.includes("$bunfs") ||
				import.meta.url.includes("~BUN") ||
				import.meta.url.includes("%7EBUN");
			const entryPath = isBunBinary ? process.execPath : fileURLToPath(import.meta.url);
			const packageDir = resolvePackageDir(entryPath, process.execPath, isBunBinary);
			const resources = resolveNativeRuntimeResources(packageDir);
			process.exitCode = await runNativeRuntime(resources, consumed.args);
			return;
		} catch (error) {
			const message = error instanceof Error ? error.message : String(error);
			if (resolved.selection === "native") {
				process.stderr.write(`Native runtime bootstrap failed: ${message}\n`);
				process.exitCode = NATIVE_BOOTSTRAP_EXIT_CODE;
				return;
			}
			// auto 只允许在 child 尚未启动的资源定位阶段回退，之后的 native 错误保持原退出码。
			process.stderr.write(`${runtimeFallbackRecord(message)}\n`);
		}
	}

	if (process.versions.bun) {
		// Bun compiled binary 无法用 process.execPath 派生 archive 内模块；仅在已选择 TS runtime 后加载。
		const { startBunTypeScriptRuntime } = await import("./cli-runtime-bun-main.ts");
		await startBunTypeScriptRuntime(consumed.args);
		return;
	}

	// Node 包派生产品进程，保证 selector 自身没有 TypeScript runtime 的依赖边。
	// 发布构建会将两个入口放入同一 dist 目录，故路径与调用 cwd 无关。
	process.exitCode = await runTypeScriptRuntime(
		fileURLToPath(new URL("./cli-runtime.js", import.meta.url)),
		consumed.args,
	);
}

void start().catch((error: unknown) => {
	process.stderr.write(`${error instanceof Error ? (error.stack ?? error.message) : String(error)}\n`);
	process.exitCode = 1;
});

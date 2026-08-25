#!/usr/bin/env node
/**
 * Node.js 的 TypeScript 产品 runtime 独立可执行入口。
 *
 * 该文件由薄选择器作为子进程启动。选择器本身不导入 Provider SDK、TUI 或 `main.ts`，因此
 * native 路径不会因为模块预加载而落入 TypeScript 产品运行时。Bun 需要的 OAuth、sandbox
 * 环境恢复和 Bedrock 注册位于独立的 `cli-runtime-bun.ts`，避免改变 Node 的初始化依赖。
 */
import { APP_NAME } from "./config.ts";
import { configureHttpDispatcher } from "./core/http-dispatcher.ts";
import { main } from "./main.ts";

/** 启动原有 Node.js TypeScript 产品运行时。 */
export async function startTypeScriptRuntime(args: string[]): Promise<void> {
	process.title = APP_NAME;
	process.env.PI_CODING_AGENT = "true";
	process.env.AI_AGENT = "pi";
	process.emitWarning = (() => {}) as typeof process.emitWarning;

	// 在 Provider SDK 可能发起请求前完成全局 dispatcher 配置。
	configureHttpDispatcher();
	await main(args);
}

void startTypeScriptRuntime(process.argv.slice(2)).catch((error: unknown) => {
	process.stderr.write(`${error instanceof Error ? (error.stack ?? error.message) : String(error)}\n`);
	process.exitCode = 1;
});

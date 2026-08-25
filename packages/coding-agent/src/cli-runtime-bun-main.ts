/**
 * Bun binary 的 TypeScript runtime 实现。
 *
 * 薄选择器仅在已经决定使用 TypeScript runtime 后动态导入本模块。这样 native sidecar 启动
 * 不会加载 OAuth、Bedrock、TUI 或主产品模块，同时 Bun archive 不需要尝试把自身当作 Node.js
 * 可执行文件来派生。
 */
import { registerBunOAuthFlows } from "@earendil-works/pi-ai/bun-oauth";
import { registerBedrockProvider } from "./bun/register-bedrock.ts";
import { restoreSandboxEnv } from "./bun/restore-sandbox-env.ts";
import { APP_NAME } from "./config.ts";
import { configureHttpDispatcher } from "./core/http-dispatcher.ts";
import { main } from "./main.ts";

/**
 * 启动 Bun 中的原有 TypeScript 产品 runtime。
 *
 * 先恢复环境变量，再注册 OAuth/Bedrock；这与旧 Bun 入口的依赖顺序等价，并确保 Provider
 * 初始化前能够读取 sandbox 恢复后的认证配置。
 */
export async function startBunTypeScriptRuntime(args: string[]): Promise<void> {
	restoreSandboxEnv();
	registerBunOAuthFlows();
	registerBedrockProvider();

	process.title = APP_NAME;
	process.env.PI_CODING_AGENT = "true";
	process.env.AI_AGENT = "pi";
	process.emitWarning = (() => {}) as typeof process.emitWarning;

	configureHttpDispatcher();
	await main(args);
}

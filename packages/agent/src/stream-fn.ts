import type { StreamFn } from "./types.ts";

let defaultStreamFn: StreamFn | undefined;

/**
 * Configure the fallback used by Agent and low-level loops when callers omit streamFn.
 *
 * Hosts that provide a default model runtime can install its stream function here
 * without making pi-agent-core depend on a provider catalog or compatibility layer.
 *
 * 配置 Agent 和底层循环在调用方未显式传入 streamFn 时使用的兜底实现。
 * 这里是 agent-core 与具体 Provider Adapter 之间的解耦边界：宿主应用可以注入默认
 * 模型流函数，而 agent-core 不需要依赖 provider catalog 或某个供应商的兼容层。
 */
export function setDefaultStreamFn(streamFn: StreamFn | undefined): void {
	defaultStreamFn = streamFn;
}

/**
 * 读取全局兜底流函数。若没有提前配置，立即失败比等到真正发起请求时才失败更容易定位。
 */
export function getDefaultStreamFn(): StreamFn {
	if (!defaultStreamFn) {
		throw new Error("No default stream function configured. Pass streamFn explicitly or call setDefaultStreamFn().");
	}
	return defaultStreamFn;
}

import { streamSimple } from "@earendil-works/pi-ai/compat";
import { builtinModels } from "@earendil-works/pi-ai/providers/all";
import type { ModelRef } from "@earendil-works/pi-protocol";
import { ProviderHost } from "./index.ts";
import { runProviderHostStdio } from "./stdio.ts";

/**
 * 启动 Provider Host 的标准输入输出服务。
 *
 * 该函数是 Node sidecar 与 Bun 编译二进制之间唯一共享的启动边界。两者均将 Provider SDK
 * 放在独立子进程中：Rust Core 只通过 framed-CBOR 与它通信，因此不能把模型注册、信号处理
 * 或 stdout 生命周期复制到两个启动器中。
 */
export function startProviderHost(): void {
	const models = builtinModels();
	const host = new ProviderHost({
		// 具体 Provider 注册与模型数据只属于可执行生产入口，核心 Host 保持为可注入边界。
		stream: streamSimple,
		modelResolver: {
			resolve(reference: ModelRef) {
				const model = models.getModel(reference.provider, reference.id);
				if (!model) {
					throw new Error(`Unknown Provider Host model: ${reference.provider}/${reference.id}`);
				}
				return model;
			},
		},
	});
	const server = runProviderHostStdio(host);
	let closing: Promise<void> | undefined;

	/**
	 * 信号和 stdin EOF 共享同一条异步关闭路径。
	 *
	 * 不能在收到信号时立即退出：活动 Provider 请求还需要写入 error/complete，Rust Core 才能
	 * 回收对应的 request ID。关闭期间的异常只写入 stderr，绝不污染 framed-CBOR stdout。
	 */
	function close(): Promise<void> {
		closing ??= server.close().catch((error: unknown) => {
			process.stderr.write(
				`Provider Host shutdown failed: ${error instanceof Error ? error.message : String(error)}\n`,
			);
			process.exitCode = 1;
		});
		return closing;
	}

	process.once("SIGINT", () => void close());
	process.once("SIGTERM", () => void close());
}

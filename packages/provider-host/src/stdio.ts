import type { Writable } from "node:stream";
import {
	createProviderHostRequestDecoder,
	encodeProviderHostResponse,
	type ProviderHostRequest,
	type ProviderHostResponse,
} from "@earendil-works/pi-protocol";
import type { ProviderHost } from "./index.ts";

/** Provider Host 进程边界使用的最小字节输入接口。 */
export interface ProviderHostByteInput {
	on(event: "data", listener: (chunk: Uint8Array) => void): unknown;
	on(event: "end", listener: () => void): unknown;
	on(event: "error", listener: (error: Error) => void): unknown;
}

/** 将已构帧的响应写入 Rust Core 的最小字节输出接口。 */
export interface ProviderHostByteOutput {
	write(chunk: Uint8Array, callback: (error?: Error | null) => void): boolean;
}

/**
 * 把一个 ProviderHost 绑定为 framed CBOR stdio 服务。
 *
 * 输入 request 后异步消费 Host 流；abort 不等待流完成，因此可以在同一连接中取消正在运行
 * 的请求。每个流无论成功、失败或取消都只写入一个 complete，供 Core 清理 request 状态。
 */
export class ProviderHostStdioServer {
	private readonly decoder = createProviderHostRequestDecoder();
	private readonly activeTasks = new Set<Promise<void>>();
	private writeTail: Promise<void> = Promise.resolve();
	private closing: Promise<void> | undefined;
	private ended = false;

	private readonly host: ProviderHost;
	private readonly output: ProviderHostByteOutput;

	constructor(host: ProviderHost, output: ProviderHostByteOutput) {
		this.host = host;
		this.output = output;
	}

	attach(input: ProviderHostByteInput): void {
		input.on("data", (chunk) => this.receive(chunk));
		input.on("end", () => void this.close());
		input.on("error", () => void this.close());
	}

	receive(chunk: Uint8Array): void {
		if (this.ended) return;
		for (const message of this.decoder.push(chunk)) this.handle(message);
	}

	/**
	 * 关闭输入并等待已排队的 framed-CBOR 输出完成。
	 *
	 * stdin EOF 是 Rust supervisor 的正常关闭信号。必须先 abort 所有 Provider 请求，再等待
	 * 每个执行任务写完它唯一的 complete；否则子进程退出会让 Core 永久保留 active request。
	 */
	close(): Promise<void> {
		if (this.closing) return this.closing;
		this.ended = true;
		this.closing = this.closeInternal();
		return this.closing;
	}

	private async closeInternal(): Promise<void> {
		let decoderError: unknown;
		try {
			this.decoder.end();
		} catch (error) {
			// 截断输入仍必须触发取消和 complete 收尾，协议错误在收尾后才向 supervisor 报告。
			decoderError = error;
		} finally {
			this.host.abortAll();
		}
		await Promise.allSettled(this.activeTasks);
		await this.writeTail;
		if (decoderError) throw decoderError;
	}

	/** 与旧的同步调用点兼容；进程入口应使用 `close` 等待完整关闭。 */
	end(): void {
		void this.close();
	}

	private handle(message: ProviderHostRequest): void {
		if (message.type === "abort") {
			this.host.abort(message.requestId);
			return;
		}
		const task = this.execute(message.request);
		this.activeTasks.add(task);
		void task.then(
			() => this.activeTasks.delete(task),
			() => this.activeTasks.delete(task),
		);
	}

	private async execute(request: Extract<ProviderHostRequest, { type: "request" }>["request"]): Promise<void> {
		try {
			for await (const event of this.host.execute(request)) {
				await this.send({ type: "event", requestId: request.requestId, event });
			}
		} catch (error) {
			const timestamp = Date.now();
			await this.send({
				type: "event",
				requestId: request.requestId,
				event: {
					type: "error",
					messageId: `provider-${timestamp}`,
					content: [],
					timestamp,
					message: messageForError(error),
				},
			});
		} finally {
			await this.send({ type: "complete", requestId: request.requestId });
		}
	}

	private send(message: ProviderHostResponse): Promise<void> {
		const frame = encodeProviderHostResponse(message);
		const write = this.writeTail.then(
			() =>
				new Promise<void>((resolve, reject) => {
					this.output.write(frame, (error) => (error ? reject(error) : resolve()));
				}),
		);
		this.writeTail = write.catch(() => {});
		return write;
	}
}

function messageForError(error: unknown): string {
	return error instanceof Error && error.message ? error.message : "Provider Host execution failed";
}

/** 启动独立 Provider Host 子进程的 stdio 服务。 */
export function runProviderHostStdio(host: ProviderHost): ProviderHostStdioServer {
	const server = new ProviderHostStdioServer(host, process.stdout as Writable);
	server.attach(process.stdin);
	return server;
}

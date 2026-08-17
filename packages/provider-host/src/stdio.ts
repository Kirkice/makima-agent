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
	private writeTail: Promise<void> = Promise.resolve();
	private ended = false;

	private readonly host: ProviderHost;
	private readonly output: ProviderHostByteOutput;

	constructor(host: ProviderHost, output: ProviderHostByteOutput) {
		this.host = host;
		this.output = output;
	}

	attach(input: ProviderHostByteInput): void {
		input.on("data", (chunk) => this.receive(chunk));
		input.on("end", () => this.end());
		input.on("error", () => this.end());
	}

	receive(chunk: Uint8Array): void {
		if (this.ended) return;
		for (const message of this.decoder.push(chunk)) this.handle(message);
	}

	end(): void {
		if (this.ended) return;
		this.decoder.end();
		this.ended = true;
	}

	private handle(message: ProviderHostRequest): void {
		if (message.type === "abort") {
			this.host.abort(message.requestId);
			return;
		}
		void this.execute(message.request);
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

import { type AssistantMessageEvent, createAssistantMessageEventStream, type Model } from "@earendil-works/pi-ai";
import { beforeEach, describe, expect, test, vi } from "vitest";
import { ProviderHost, type ProviderStreamFactory } from "../src/index.ts";

const request = {
	requestId: "request-1",
	model: { provider: "test-provider", id: "test-model" },
	systemPrompt: "遵循系统提示",
	messages: [
		{
			id: "user-1",
			role: "user" as const,
			content: [{ type: "text" as const, text: "你好" }],
			timestamp: 1,
		},
	],
	tools: [
		{
			name: "echo",
			description: "回显文本",
			inputSchema: { type: "object", properties: { text: { type: "string" } } },
		},
	],
};

function model(): Model<"openai-completions"> {
	return {
		id: "test-model",
		name: "Test model",
		api: "openai-completions",
		provider: "test-provider",
		baseUrl: "https://provider.invalid",
		reasoning: false,
		input: ["text"],
		cost: { input: 0, output: 0, cacheRead: 0, cacheWrite: 0 },
		contextWindow: 8_192,
		maxTokens: 1_024,
	};
}

function stream(events: AssistantMessageEvent[]): ReturnType<typeof createAssistantMessageEventStream> {
	const result = createAssistantMessageEventStream();
	queueMicrotask(() => {
		for (const event of events) result.push(event);
	});
	return result;
}

function assistant(timestamp: number) {
	return {
		role: "assistant" as const,
		content: [],
		api: "openai-completions" as const,
		provider: "test-provider",
		model: "test-model",
		usage: {
			input: 1,
			output: 1,
			cacheRead: 0,
			cacheWrite: 0,
			totalTokens: 2,
			cost: { input: 0, output: 0, cacheRead: 0, cacheWrite: 0, total: 0 },
		},
		stopReason: "stop" as const,
		timestamp,
	};
}

async function collect(host: ProviderHost, input: unknown) {
	const events = [];
	for await (const event of host.execute(input)) events.push(event);
	return events;
}

describe("ProviderHost", () => {
	beforeEach(() => vi.clearAllMocks());

	test("解析请求、解析完整模型，并投影真实 SDK 流", async () => {
		const resolver = vi.fn(() => model());
		const factory = vi.fn(() =>
			stream([
				{ type: "start", partial: { ...assistant(10), responseId: "response-1" } },
				{ type: "text_delta", contentIndex: 0, delta: "你好", partial: assistant(10) },
				{ type: "done", reason: "stop", message: assistant(11) },
			]),
		);
		const host = new ProviderHost({ modelResolver: { resolve: resolver }, stream: factory });

		expect(await collect(host, request)).toEqual([
			{ type: "start", messageId: "response-1", timestamp: 10 },
			{ type: "text_delta", contentIndex: 0, delta: "你好" },
			{ type: "done", timestamp: 11, stopReason: "stop" },
		]);
		expect(resolver).toHaveBeenCalledWith(request.model);
		expect(factory).toHaveBeenCalledWith(
			model(),
			expect.objectContaining({
				systemPrompt: request.systemPrompt,
				messages: [expect.objectContaining({ role: "user" })],
			}),
			expect.objectContaining({ signal: expect.any(AbortSignal) }),
		);
	});

	test("取消会转发同一个 AbortSignal，并在 SDK 抛错时产生稳定错误", async () => {
		let signal: AbortSignal | undefined;
		const factory: ProviderStreamFactory = (_model, _context, options) => {
			signal = options.signal;
			return {
				async *[Symbol.asyncIterator]() {
					await new Promise<void>((resolve) =>
						options.signal?.addEventListener("abort", () => resolve(), { once: true }),
					);
					throw new Error("SDK should report this as abort");
				},
			} as unknown as ReturnType<typeof createAssistantMessageEventStream>;
		};
		const host = new ProviderHost({ modelResolver: { resolve: model }, stream: factory, now: () => 99 });
		const execution = collect(host, request);
		await vi.waitFor(() => expect(host.activeRequestCount).toBe(1));

		expect(host.abort(request.requestId)).toBe(true);
		expect(signal?.aborted).toBe(true);
		expect(await execution).toEqual([{ type: "error", timestamp: 99, message: "Operation aborted" }]);
		expect(host.abort(request.requestId)).toBe(false);
	});

	test("拒绝重复 requestId，并把 Provider 异常归一化", async () => {
		let release: (() => void) | undefined;
		const pendingFactory: ProviderStreamFactory = () =>
			({
				async *[Symbol.asyncIterator]() {
					await new Promise<void>((resolve) => {
						release = resolve;
					});
				},
			}) as unknown as ReturnType<typeof createAssistantMessageEventStream>;
		const host = new ProviderHost({ modelResolver: { resolve: model }, stream: pendingFactory, now: () => 7 });
		const pending = collect(host, request);
		await vi.waitFor(() => expect(host.activeRequestCount).toBe(1));
		expect(await collect(host, request)).toEqual([
			{ type: "error", timestamp: 7, message: "Provider request is already active: request-1" },
		]);
		release?.();
		await pending;

		const failing = new ProviderHost({
			modelResolver: { resolve: model },
			stream: () => {
				throw new Error("网络中断");
			},
			now: () => 8,
		});
		expect(await collect(failing, { ...request, requestId: "request-2" })).toEqual([
			{ type: "error", timestamp: 8, message: "网络中断" },
		]);
	});

	test("默认超时会取消 Provider 并输出稳定错误", async () => {
		const factory: ProviderStreamFactory = (_model, _context, options) =>
			({
				async *[Symbol.asyncIterator]() {
					await new Promise<void>((resolve) =>
						options.signal?.addEventListener("abort", () => resolve(), { once: true }),
					);
					throw new Error("Provider interrupted after timeout");
				},
			}) as unknown as ReturnType<typeof createAssistantMessageEventStream>;
		const host = new ProviderHost({
			modelResolver: { resolve: model },
			stream: factory,
			defaultTimeoutMs: 1,
			now: () => 10,
		});

		expect(await collect(host, { ...request, requestId: "request-3" })).toEqual([
			{ type: "error", timestamp: 10, message: "Operation aborted" },
		]);
	});
});

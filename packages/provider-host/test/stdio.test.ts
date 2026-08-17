import { type AssistantMessageEvent, createAssistantMessageEventStream, type Model } from "@earendil-works/pi-ai";
import {
	createProviderHostResponseDecoder,
	encodeProviderHostRequest,
	type ProviderHostResponse,
} from "@earendil-works/pi-protocol";
import { describe, expect, test, vi } from "vitest";
import { ProviderHost, type ProviderStreamFactory } from "../src/index.ts";
import { type ProviderHostByteOutput, ProviderHostStdioServer } from "../src/stdio.ts";

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

function request(requestId = "request-1") {
	return {
		requestId,
		model: { provider: "test-provider", id: "test-model" },
		systemPrompt: "system",
		messages: [],
		tools: [],
	};
}

function stream(events: AssistantMessageEvent[]): ReturnType<typeof createAssistantMessageEventStream> {
	const result = createAssistantMessageEventStream();
	queueMicrotask(() => {
		for (const event of events) result.push(event);
	});
	return result;
}

class Output implements ProviderHostByteOutput {
	readonly chunks: Uint8Array[] = [];

	write(chunk: Uint8Array, callback: (error?: Error | null) => void): boolean {
		this.chunks.push(chunk);
		callback();
		return true;
	}

	responses(): ProviderHostResponse[] {
		const decoder = createProviderHostResponseDecoder();
		const responses = this.chunks.flatMap((chunk) => decoder.push(chunk));
		decoder.end();
		return responses;
	}
}

describe("ProviderHostStdioServer", () => {
	test("以 framed CBOR 投影 provider stream 并在结束时只发送一个 complete", async () => {
		const factory = vi.fn(() =>
			stream([
				{
					type: "start",
					partial: {
						role: "assistant",
						content: [],
						api: "openai-completions",
						provider: "test-provider",
						model: "test-model",
						usage: {
							input: 0,
							output: 0,
							cacheRead: 0,
							cacheWrite: 0,
							totalTokens: 0,
							cost: { input: 0, output: 0, cacheRead: 0, cacheWrite: 0, total: 0 },
						},
						stopReason: "stop",
						timestamp: 1,
						responseId: "assistant-1",
					},
				},
				{
					type: "done",
					reason: "stop",
					message: {
						role: "assistant",
						content: [],
						api: "openai-completions",
						provider: "test-provider",
						model: "test-model",
						responseId: "assistant-1",
						responseModel: "test-model",
						usage: {
							input: 0,
							output: 0,
							cacheRead: 0,
							cacheWrite: 0,
							totalTokens: 0,
							cost: { input: 0, output: 0, cacheRead: 0, cacheWrite: 0, total: 0 },
						},
						stopReason: "stop",
						timestamp: 2,
					},
				},
			]),
		);
		const output = new Output();
		const server = new ProviderHostStdioServer(
			new ProviderHost({ modelResolver: { resolve: model }, stream: factory }),
			output,
		);
		const frame = encodeProviderHostRequest({ type: "request", request: request() });

		server.receive(frame.subarray(0, 3));
		server.receive(frame.subarray(3));
		await vi.waitFor(() => expect(output.responses()).toHaveLength(3));

		expect(output.responses()).toEqual([
			{ type: "event", requestId: "request-1", event: { type: "start", messageId: "assistant-1", timestamp: 1 } },
			{
				type: "event",
				requestId: "request-1",
				event: {
					type: "done",
					messageId: "assistant-1",
					content: [],
					responseModel: "test-model",
					usage: {
						input: 0,
						output: 0,
						cacheRead: 0,
						cacheWrite: 0,
						totalTokens: 0,
						cost: { input: 0, output: 0, cacheRead: 0, cacheWrite: 0, total: 0 },
					},
					timestamp: 2,
					stopReason: "stop",
				},
			},
			{ type: "complete", requestId: "request-1" },
		]);
	});

	test("abort 可在 request stream 等待时到达，并且 complete 在 error 之后发出", async () => {
		let release: (() => void) | undefined;
		const factory: ProviderStreamFactory = (_model, _context, options) =>
			({
				async *[Symbol.asyncIterator]() {
					await new Promise<void>((resolve) => {
						release = resolve;
						options.signal?.addEventListener("abort", () => resolve(), { once: true });
					});
					throw new Error("aborted");
				},
			}) as unknown as ReturnType<typeof createAssistantMessageEventStream>;
		const output = new Output();
		const server = new ProviderHostStdioServer(
			new ProviderHost({ modelResolver: { resolve: model }, stream: factory, now: () => 7 }),
			output,
		);

		server.receive(encodeProviderHostRequest({ type: "request", request: request() }));
		await vi.waitFor(() => expect(release).toBeTypeOf("function"));
		server.receive(encodeProviderHostRequest({ type: "abort", requestId: "request-1" }));
		await vi.waitFor(() => expect(output.responses()).toHaveLength(2));

		expect(output.responses()).toEqual([
			{
				type: "event",
				requestId: "request-1",
				event: {
					type: "error",
					messageId: "provider-7",
					content: [],
					timestamp: 7,
					message: "Operation aborted",
				},
			},
			{ type: "complete", requestId: "request-1" },
		]);
	});
});

import { describe, expect, test } from "vitest";
import { mapAssistantMessageEvent, type ProviderHostAssistantMessageEvent } from "../src/index.ts";

const usage = {
	input: 10,
	output: 5,
	cacheRead: 2,
	cacheWrite: 1,
	reasoning: 3,
	totalTokens: 18,
	cost: { input: 0.01, output: 0.01, cacheRead: 0.002, cacheWrite: 0.002, total: 0.024 },
};

const terminalMessage = {
	responseId: "response-1",
	content: [
		{ type: "text" as const, text: "你好" },
		{ type: "thinking" as const, thinking: "分析", redacted: true },
		{ type: "toolCall" as const, toolCallId: "call-1", toolName: "echo", input: { value: "hello" } },
	],
	responseModel: "resolved-model",
	usage,
	timestamp: 2,
};

describe("Provider Host adapter", () => {
	test("projects TypeScript Provider events and the authoritative terminal snapshot", () => {
		const events: ProviderHostAssistantMessageEvent[] = [
			{ type: "start", partial: { responseId: "response-1", timestamp: 1 } },
			{ type: "text_start", contentIndex: 0 },
			{ type: "text_delta", contentIndex: 0, delta: "你" },
			{
				type: "thinking_delta",
				contentIndex: 1,
				delta: "分析",
				partial: { content: [{ type: "text" }, { type: "thinking", redacted: true }] },
			},
			{ type: "toolcall_delta", contentIndex: 2, delta: '{"value":"hello"}' },
			{
				type: "toolcall_end",
				contentIndex: 2,
				toolCall: { id: "call-1", name: "echo", arguments: { value: "hello" } },
			},
			{ type: "done", reason: "toolUse", message: terminalMessage },
		];

		expect(events.map(mapAssistantMessageEvent).filter((event) => event !== undefined)).toEqual([
			{ type: "start", messageId: "response-1", timestamp: 1 },
			{ type: "text_delta", contentIndex: 0, delta: "你" },
			{ type: "thinking_delta", contentIndex: 1, delta: "分析", redacted: true },
			{ type: "tool_call_delta", contentIndex: 2, delta: '{"value":"hello"}' },
			{
				type: "tool_call_end",
				contentIndex: 2,
				toolCall: { toolCallId: "call-1", toolName: "echo", input: { value: "hello" } },
			},
			{
				type: "done",
				messageId: "response-1",
				content: terminalMessage.content,
				responseModel: "resolved-model",
				usage,
				timestamp: 2,
				stopReason: "toolUse",
			},
		]);
	});

	test("maps cancellation, provider errors, and unsupported deferred responses without losing terminal data", () => {
		const aborted = { content: [{ type: "text" as const, text: "部分" }], timestamp: 3 };
		expect(mapAssistantMessageEvent({ type: "error", reason: "aborted", error: aborted })).toEqual({
			type: "error",
			messageId: "provider-3",
			content: aborted.content,
			timestamp: 3,
			message: "Operation aborted",
		});

		const failed = { ...terminalMessage, timestamp: 4, errorMessage: "网络中断" };
		expect(mapAssistantMessageEvent({ type: "error", reason: "error", error: failed })).toEqual({
			type: "error",
			messageId: "response-1",
			content: terminalMessage.content,
			responseModel: "resolved-model",
			usage,
			timestamp: 4,
			message: "网络中断",
		});

		expect(mapAssistantMessageEvent({ type: "done", reason: "deferred", message: terminalMessage })).toEqual({
			type: "error",
			messageId: "response-1",
			content: terminalMessage.content,
			responseModel: "resolved-model",
			usage,
			timestamp: 2,
			message: "Provider deferred response is not supported by the Rust Core yet.",
		});
	});
});

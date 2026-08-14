import { describe, expect, test } from "vitest";
import { mapAssistantMessageEvent, type ProviderHostAssistantMessageEvent } from "../src/index.ts";

describe("Provider Host adapter", () => {
	test("projects TypeScript Provider events into the shared stream DTO", () => {
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
			{ type: "done", reason: "toolUse", message: { timestamp: 2 } },
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
			{ type: "done", timestamp: 2, stopReason: "toolUse" },
		]);
	});

	test("maps cancellation, provider errors, and unsupported deferred responses to errors", () => {
		expect(
			mapAssistantMessageEvent({
				type: "error",
				reason: "aborted",
				error: { timestamp: 3 },
			}),
		).toEqual({ type: "error", timestamp: 3, message: "Operation aborted" });

		expect(
			mapAssistantMessageEvent({
				type: "error",
				reason: "error",
				error: { timestamp: 4, errorMessage: "网络中断" },
			}),
		).toEqual({ type: "error", timestamp: 4, message: "网络中断" });

		expect(
			mapAssistantMessageEvent({
				type: "done",
				reason: "deferred",
				message: { timestamp: 5 },
			}),
		).toEqual({
			type: "error",
			timestamp: 5,
			message: "Provider deferred response is not supported by the Rust Core yet.",
		});
	});
});

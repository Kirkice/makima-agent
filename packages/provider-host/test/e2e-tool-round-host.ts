import {
	type AssistantMessage,
	type AssistantMessageEvent,
	type Context,
	createAssistantMessageEventStream,
	type Model,
} from "@earendil-works/pi-ai";
import { ProviderHost, type ProviderStreamFactory } from "../src/index.ts";
import { runProviderHostStdio } from "../src/stdio.ts";

const model: Model<"openai-completions"> = {
	id: "model",
	name: "Rust E2E model",
	api: "openai-completions",
	provider: "test",
	baseUrl: "https://provider.invalid",
	reasoning: true,
	input: ["text"],
	cost: { input: 0, output: 0, cacheRead: 0, cacheWrite: 0 },
	contextWindow: 8_192,
	maxTokens: 1_024,
};

const emptyUsage = {
	input: 0,
	output: 0,
	cacheRead: 0,
	cacheWrite: 0,
	totalTokens: 0,
	cost: { input: 0, output: 0, cacheRead: 0, cacheWrite: 0, total: 0 },
};

const terminalUsage = {
	input: 10,
	output: 5,
	cacheRead: 2,
	cacheWrite: 1,
	reasoning: 3,
	totalTokens: 21,
	cost: { input: 0.01, output: 0.02, cacheRead: 0.003, cacheWrite: 0.004, total: 0.037 },
};

function assistant(
	content: AssistantMessage["content"],
	stopReason: AssistantMessage["stopReason"],
	timestamp: number,
	responseId: string,
	usage: AssistantMessage["usage"] = emptyUsage,
): AssistantMessage {
	return {
		role: "assistant",
		content,
		api: model.api,
		provider: model.provider,
		model: model.id,
		responseId,
		responseModel: "resolved-thinking-model",
		usage,
		stopReason,
		timestamp,
	};
}

function eventStream(events: AssistantMessageEvent[]) {
	const stream = createAssistantMessageEventStream();
	queueMicrotask(() => {
		for (const event of events) stream.push(event);
	});
	return stream;
}

function assertFirstRequest(context: Context): void {
	if (context.messages.length !== 1 || context.messages[0]?.role !== "user") {
		throw new Error("First Provider request must contain exactly one user message");
	}
	if (context.tools?.length !== 1 || context.tools[0]?.name !== "read") {
		throw new Error("First Provider request must expose the Rust read tool");
	}
}

function assertContinuation(context: Context): void {
	if (context.messages.length !== 4) {
		throw new Error(
			`Continuation must contain user, assistant and two tool messages; received ${context.messages.length}`,
		);
	}
	const assistantMessage = context.messages[1];
	if (
		assistantMessage?.role !== "assistant" ||
		assistantMessage.content[0]?.type !== "toolCall" ||
		assistantMessage.content[0].id !== "read-call-1" ||
		assistantMessage.content[1]?.type !== "toolCall" ||
		assistantMessage.content[1].id !== "read-call-2"
	) {
		throw new Error("Continuation must replay both assistant tool calls in source order");
	}

	const expectedResults = [
		["read-call-1", "hello from the first Rust read tool call"],
		["read-call-2", "hello from the second Rust read tool call"],
	] as const;
	for (const [index, [toolCallId, text]] of expectedResults.entries()) {
		const toolResult = context.messages[index + 2];
		if (
			toolResult?.role !== "toolResult" ||
			toolResult.toolCallId !== toolCallId ||
			toolResult.isError ||
			toolResult.content[0]?.type !== "text" ||
			toolResult.content[0].text !== text
		) {
			throw new Error(`Continuation must contain the successful ${toolCallId} Rust read result`);
		}
	}
}

const stream: ProviderStreamFactory = (_resolvedModel, context) => {
	if (context.messages.length === 1) {
		assertFirstRequest(context);
		const partial = assistant([], "pending", 101, "assistant-tool");
		const firstToolCall = {
			type: "toolCall" as const,
			id: "read-call-1",
			name: "read",
			arguments: { path: "first.txt" },
		};
		const secondToolCall = {
			type: "toolCall" as const,
			id: "read-call-2",
			name: "read",
			arguments: { path: "second.txt" },
		};
		return eventStream([
			{ type: "start", partial },
			{ type: "toolcall_delta", contentIndex: 0, delta: '{"path":"first.txt"}', partial },
			{ type: "toolcall_end", contentIndex: 0, toolCall: firstToolCall, partial },
			{ type: "toolcall_delta", contentIndex: 1, delta: '{"path":"second.txt"}', partial },
			{ type: "toolcall_end", contentIndex: 1, toolCall: secondToolCall, partial },
			{
				type: "done",
				reason: "toolUse",
				message: assistant([firstToolCall, secondToolCall], "toolUse", 102, "assistant-tool"),
			},
		]);
	}

	assertContinuation(context);
	const partial = assistant([], "pending", 103, "assistant-final");
	return eventStream([
		{ type: "start", partial },
		{
			type: "thinking_delta",
			contentIndex: 0,
			delta: "draft reasoning",
			partial: assistant([{ type: "thinking", thinking: "draft reasoning" }], "pending", 103, "assistant-final"),
		},
		{
			type: "text_delta",
			contentIndex: 1,
			delta: "draft answer",
			partial: assistant(
				[
					{ type: "thinking", thinking: "draft reasoning" },
					{ type: "text", text: "draft answer" },
				],
				"pending",
				103,
				"assistant-final",
			),
		},
		{
			type: "done",
			reason: "stop",
			message: assistant(
				[
					{ type: "thinking", thinking: "final reasoning" },
					{ type: "text", text: "Rust runtime completed the tool round" },
				],
				"stop",
				104,
				"assistant-final",
				terminalUsage,
			),
		},
	]);
};

runProviderHostStdio(
	new ProviderHost({
		modelResolver: { resolve: () => model },
		stream,
	}),
);

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
	reasoning: false,
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

function assistant(
	content: AssistantMessage["content"],
	stopReason: AssistantMessage["stopReason"],
	timestamp: number,
	responseId: string,
): AssistantMessage {
	return {
		role: "assistant",
		content,
		api: model.api,
		provider: model.provider,
		model: model.id,
		responseId,
		usage: emptyUsage,
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
	if (context.messages.length !== 3) {
		throw new Error(
			`Continuation must contain user, assistant and tool messages; received ${context.messages.length}`,
		);
	}
	const assistantMessage = context.messages[1];
	if (assistantMessage?.role !== "assistant" || assistantMessage.content[0]?.type !== "toolCall") {
		throw new Error("Continuation must replay the assistant tool call");
	}
	const toolResult = context.messages[2];
	if (
		toolResult?.role !== "toolResult" ||
		toolResult.toolCallId !== "read-call-1" ||
		toolResult.isError ||
		toolResult.content[0]?.type !== "text" ||
		toolResult.content[0].text !== "hello from the Rust read tool"
	) {
		throw new Error("Continuation must contain the successful Rust read result");
	}
}

const stream: ProviderStreamFactory = (_resolvedModel, context) => {
	if (context.messages.length === 1) {
		assertFirstRequest(context);
		const partial = assistant([], "pending", 101, "assistant-tool");
		const toolCall = { type: "toolCall" as const, id: "read-call-1", name: "read", arguments: { path: "hello.txt" } };
		return eventStream([
			{ type: "start", partial },
			{ type: "toolcall_delta", contentIndex: 0, delta: '{"path":"hello.txt"}', partial },
			{ type: "toolcall_end", contentIndex: 0, toolCall, partial },
			{
				type: "done",
				reason: "toolUse",
				message: assistant([toolCall], "toolUse", 102, "assistant-tool"),
			},
		]);
	}

	assertContinuation(context);
	const partial = assistant([], "pending", 103, "assistant-final");
	const text = "Rust runtime completed the tool round";
	return eventStream([
		{ type: "start", partial },
		{ type: "text_delta", contentIndex: 0, delta: text, partial },
		{ type: "done", reason: "stop", message: assistant([{ type: "text", text }], "stop", 104, "assistant-final") },
	]);
};

runProviderHostStdio(
	new ProviderHost({
		modelResolver: { resolve: () => model },
		stream,
	}),
);

import type { AssistantContent, JsonValue, ProviderStreamEvent, ToolCall, Usage } from "./schemas.ts";

/**
 * TypeScript Provider Host 在协议边界需要读取的最小 assistant stream 字段。
 *
 * 该类型刻意不直接依赖 `@earendil-works/pi-ai`：Provider Host 可将 SDK 的
 * `AssistantMessageEvent` 以结构兼容方式传入，同时共享协议包仍保持独立、可发布。
 */
interface ProviderHostTerminalMessage {
	responseId?: string;
	content: AssistantContent[];
	responseModel?: string;
	usage?: Usage;
	timestamp: number;
	errorMessage?: string;
}

export type ProviderHostAssistantMessageEvent =
	| { type: "start"; partial: { responseId?: string; timestamp: number } }
	| { type: "text_start"; contentIndex: number }
	| { type: "text_delta"; contentIndex: number; delta: string }
	| { type: "text_end"; contentIndex: number; content: string }
	| { type: "thinking_start"; contentIndex: number }
	| { type: "thinking_delta"; contentIndex: number; delta: string; partial: { content: ProviderHostContent[] } }
	| { type: "thinking_end"; contentIndex: number; content: string }
	| { type: "toolcall_start"; contentIndex: number }
	| { type: "toolcall_delta"; contentIndex: number; delta: string }
	| {
			type: "toolcall_end";
			contentIndex: number;
			toolCall: { id: string; name: string; arguments: JsonValue };
	  }
	| {
			type: "done";
			reason: "stop" | "length" | "toolUse" | "deferred";
			message: ProviderHostTerminalMessage;
	  }
	| {
			type: "error";
			reason: "aborted" | "error";
			error: ProviderHostTerminalMessage;
	  };

type ProviderHostContent = { type: "thinking"; redacted?: boolean } | { type: string; redacted?: boolean };

/**
 * 把 TypeScript Provider SDK 已归一化的事件投影为 Rust Core 可消费的稳定 DTO。
 *
 * 内容块的 start/end 仅描述 SDK 内部缓冲生命周期，因此不会产生跨进程事件。增量用于
 * 实时 progress；最终消息快照是 transcript 的权威值，可覆盖 Provider 在最后一个 chunk
 * 对内容、usage 或 responseModel 的修正。`deferred` 尚未进入 Rust Core 的恢复流程，
 * 明确转换为稳定错误，避免把未实现语义伪装成正常完成。
 */
export function mapAssistantMessageEvent(event: ProviderHostAssistantMessageEvent): ProviderStreamEvent | undefined {
	switch (event.type) {
		case "start":
			return {
				type: "start",
				messageId: event.partial.responseId ?? `provider-${event.partial.timestamp}`,
				timestamp: event.partial.timestamp,
			};
		case "text_delta":
			return { type: "text_delta", contentIndex: event.contentIndex, delta: event.delta };
		case "thinking_delta":
			return {
				type: "thinking_delta",
				contentIndex: event.contentIndex,
				delta: event.delta,
				redacted: thinkingIsRedacted(event.partial.content, event.contentIndex),
			};
		case "toolcall_delta":
			return { type: "tool_call_delta", contentIndex: event.contentIndex, delta: event.delta };
		case "toolcall_end":
			return {
				type: "tool_call_end",
				contentIndex: event.contentIndex,
				toolCall: toolCallFromProvider(event.toolCall),
			};
		case "done":
			return event.reason === "deferred"
				? terminalError(event.message, "Provider deferred response is not supported by the Rust Core yet.")
				: {
						type: "done",
						messageId: terminalMessageId(event.message),
						content: event.message.content,
						...(event.message.responseModel === undefined ? {} : { responseModel: event.message.responseModel }),
						usage: requiredTerminalUsage(event.message),
						timestamp: event.message.timestamp,
						stopReason: event.reason,
					};
		case "error":
			return terminalError(
				event.error,
				event.error.errorMessage ?? (event.reason === "aborted" ? "Operation aborted" : "Provider request failed"),
			);
		case "text_start":
		case "text_end":
		case "thinking_start":
		case "thinking_end":
		case "toolcall_start":
			return undefined;
	}
}

function terminalMessageId(message: ProviderHostTerminalMessage): string {
	return message.responseId ?? `provider-${message.timestamp}`;
}

function requiredTerminalUsage(message: ProviderHostTerminalMessage): Usage {
	if (message.usage === undefined) throw new TypeError("Successful Provider terminal message must include usage");
	return message.usage;
}

function terminalError(message: ProviderHostTerminalMessage, errorMessage: string): ProviderStreamEvent {
	return {
		type: "error",
		messageId: terminalMessageId(message),
		content: message.content,
		...(message.responseModel === undefined ? {} : { responseModel: message.responseModel }),
		...(message.usage === undefined ? {} : { usage: message.usage }),
		timestamp: message.timestamp,
		message: errorMessage,
	};
}

function thinkingIsRedacted(content: ProviderHostContent[], contentIndex: number): boolean | undefined {
	const block = content[contentIndex];
	return block?.type === "thinking" ? block.redacted : undefined;
}

function toolCallFromProvider(toolCall: { id: string; name: string; arguments: JsonValue }): ToolCall {
	return {
		toolCallId: toolCall.id,
		toolName: toolCall.name,
		input: toolCall.arguments,
	};
}

import type {
	Api,
	AssistantMessage,
	AssistantMessageEvent,
	AssistantMessageEventStream,
	Context,
	Message,
	Model,
	SimpleStreamOptions,
	Tool,
	ToolResultMessage,
	Usage,
	UserMessage,
} from "@earendil-works/pi-ai";
import {
	type JsonValue,
	type ModelRef,
	mapAssistantMessageEvent,
	type ProviderRequest,
	type ProviderStreamEvent,
	parseProviderRequest,
	type TranscriptItem,
} from "@earendil-works/pi-protocol";
import Type, { type TSchema } from "typebox";

/** 根据 Rust Core 传来的稳定模型引用定位 Host 私有的完整模型配置。 */
export interface ProviderModelResolver {
	resolve(model: ModelRef): Model<Api> | Promise<Model<Api>>;
}

/**
 * 将已完成的共享 transcript 映射为 Provider SDK 的上下文。
 *
 * 协议中不存在的 SDK 专用字段（provider 原始响应、签名与诊断）不可被伪造；它们在重放时
 * 自然缺失，Provider adapter 仍应按共享的文本、thinking、工具调用与工具结果工作。
 */
export interface ProviderContextConverter {
	toContext(request: ProviderRequest): Context;
}

/**
 * Provider SDK 流端口。
 *
 * 核心 Host 不导入具体 Provider catalog；生产入口负责注入 `streamSimple`，测试和嵌入场景可注入
 * 确定性实现。这样仅加载协议转换与 stdio 边界时，不会触发模型数据或 Provider 注册副作用。
 */
export type ProviderStreamFactory = (
	model: Model<Api>,
	context: Context,
	options: SimpleStreamOptions,
) => AssistantMessageEventStream;

export interface ProviderHostOptions {
	modelResolver: ProviderModelResolver;
	contextConverter?: ProviderContextConverter;
	stream: ProviderStreamFactory;
	requestOptions?: Omit<SimpleStreamOptions, "signal">;
	defaultTimeoutMs?: number;
	now?: () => number;
}

/**
 * 真实 Provider SDK 的异步执行边界。
 *
 * 每个 requestId 绑定一个独立 AbortController。取消会立即转发给 pi-ai Provider；无论是 SDK
 * 主动发送 aborted 事件、抛出异常或超时，Host 都只向 Rust Core 输出稳定的 `error` DTO。
 */
export class ProviderHost {
	private readonly modelResolver: ProviderModelResolver;
	private readonly contextConverter: ProviderContextConverter;
	private readonly stream: ProviderStreamFactory;
	private readonly requestOptions: Omit<SimpleStreamOptions, "signal">;
	private readonly defaultTimeoutMs: number | undefined;
	private readonly now: () => number;
	private readonly active = new Map<string, AbortController>();

	constructor(options: ProviderHostOptions) {
		this.modelResolver = options.modelResolver;
		this.contextConverter = options.contextConverter ?? defaultProviderContextConverter;
		this.stream = options.stream;
		this.requestOptions = options.requestOptions ?? {};
		this.defaultTimeoutMs = options.defaultTimeoutMs;
		this.now = options.now ?? Date.now;
	}

	/** 执行并投影一次请求；同一 requestId 同时只能存在一个活动流。 */
	async *execute(input: unknown): AsyncGenerator<ProviderStreamEvent> {
		const request = parseProviderRequest(input);
		if (this.active.has(request.requestId)) {
			yield this.error(`Provider request is already active: ${request.requestId}`);
			return;
		}

		const controller = new AbortController();
		this.active.set(request.requestId, controller);
		const timeout = this.scheduleTimeout(controller);
		try {
			const model = await this.modelResolver.resolve(request.model);
			const context = this.contextConverter.toContext(request);
			const response = this.stream(model, context, { ...this.requestOptions, signal: controller.signal });
			for await (const event of response) {
				const mapped = mapAiAssistantMessageEvent(event);
				if (mapped) yield mapped;
			}
		} catch (error) {
			yield this.error(messageForError(error, controller.signal.aborted));
		} finally {
			if (timeout !== undefined) clearTimeout(timeout);
			this.active.delete(request.requestId);
		}
	}

	/** 中止指定请求；不存在的 requestId 返回 false，调用方可安全重试取消。 */
	abort(requestId: string): boolean {
		const controller = this.active.get(requestId);
		if (!controller) return false;
		controller.abort();
		return true;
	}

	get activeRequestCount(): number {
		return this.active.size;
	}

	private scheduleTimeout(controller: AbortController): ReturnType<typeof setTimeout> | undefined {
		if (this.defaultTimeoutMs === undefined) return undefined;
		if (!Number.isSafeInteger(this.defaultTimeoutMs) || this.defaultTimeoutMs <= 0) return undefined;
		return setTimeout(() => controller.abort(), this.defaultTimeoutMs);
	}

	private error(message: string): ProviderStreamEvent {
		return { type: "error", timestamp: this.now(), message };
	}
}

export const defaultProviderContextConverter: ProviderContextConverter = {
	toContext(request) {
		return {
			systemPrompt: request.systemPrompt,
			messages: request.messages.map(toAiMessage),
			tools: request.tools.map(
				(tool) =>
					({
						name: tool.name,
						description: tool.description,
						parameters: toolParameters(tool.inputSchema),
					}) satisfies Tool,
			),
		};
	},
};

/**
 * 协议只保证 inputSchema 是 JSON 值；pi-ai 工具参数必须是 JSON Schema 对象。
 * 在 SDK 边界显式拒绝不合法形态，避免把不可执行的 schema 静默发送给 Provider。
 */
function toolParameters(inputSchema: JsonValue): TSchema {
	if (!isJsonObject(inputSchema)) throw new TypeError("Provider tool inputSchema must be a JSON Schema object");
	return Type.Unsafe(inputSchema as unknown as TSchema);
}

function toAiMessage(item: TranscriptItem): Message {
	switch (item.role) {
		case "user":
			return toAiUserMessage(item);
		case "assistant":
			return toAiAssistantMessage(item);
		case "tool":
			return toAiToolResultMessage(item);
	}
}

function toAiUserMessage(item: Extract<TranscriptItem, { role: "user" }>): UserMessage {
	return {
		role: "user",
		content: item.content.map((content) =>
			content.type === "text"
				? { type: "text", text: content.text }
				: { type: "image", data: content.data, mimeType: content.mimeType },
		),
		timestamp: item.timestamp,
	};
}

function toAiAssistantMessage(item: Extract<TranscriptItem, { role: "assistant" }>): AssistantMessage {
	const content: AssistantMessage["content"] = item.content.map((part) =>
		part.type === "text"
			? { type: "text", text: part.text }
			: part.type === "thinking"
				? {
						type: "thinking",
						thinking: part.thinking,
						...(part.redacted === undefined ? {} : { redacted: part.redacted }),
					}
				: {
						type: "toolCall",
						id: part.toolCallId,
						name: part.toolName,
						arguments: toolCallArguments(part.input),
					},
	);
	return {
		role: "assistant",
		content,
		api: item.model.provider,
		provider: item.model.provider,
		model: item.model.id,
		...(item.responseModel === undefined ? {} : { responseModel: item.responseModel }),
		usage: toAiUsage(item.usage),
		stopReason: item.status === "streaming" ? "pending" : item.stopReason,
		...(item.status === "error" || item.status === "aborted"
			? item.errorMessage === undefined
				? {}
				: { errorMessage: item.errorMessage }
			: {}),
		timestamp: item.timestamp,
	};
}

function toAiToolResultMessage(item: Extract<TranscriptItem, { role: "tool" }>): ToolResultMessage<JsonValue> {
	return {
		role: "toolResult",
		toolCallId: item.toolCallId,
		toolName: item.toolName,
		content: item.content.map((content) =>
			content.type === "text"
				? { type: "text", text: content.text }
				: { type: "image", data: content.data, mimeType: content.mimeType },
		),
		...(item.details === undefined ? {} : { details: item.details }),
		...(item.usage === undefined ? {} : { usage: toAiUsage(item.usage) }),
		isError: item.isError,
		timestamp: item.timestamp,
	};
}

function toolCallArguments(input: JsonValue): Record<string, JsonValue> {
	if (!isJsonObject(input)) throw new TypeError("Provider tool call input must be a JSON object");
	return input;
}

function isJsonObject(value: JsonValue): value is Record<string, JsonValue> {
	return value !== null && !Array.isArray(value) && typeof value === "object";
}

function toAiUsage(usage: Extract<TranscriptItem, { role: "assistant" }>["usage"]): Usage {
	return (
		usage ?? {
			input: 0,
			output: 0,
			cacheRead: 0,
			cacheWrite: 0,
			totalTokens: 0,
			cost: { input: 0, output: 0, cacheRead: 0, cacheWrite: 0, total: 0 },
		}
	);
}

function mapAiAssistantMessageEvent(event: AssistantMessageEvent): ProviderStreamEvent | undefined {
	switch (event.type) {
		case "start":
			return mapAssistantMessageEvent({
				type: "start",
				partial: { responseId: event.partial.responseId, timestamp: event.partial.timestamp },
			});
		case "text_start":
		case "text_end":
		case "thinking_start":
		case "thinking_end":
		case "toolcall_start":
			return undefined;
		case "text_delta":
			return mapAssistantMessageEvent({ type: "text_delta", contentIndex: event.contentIndex, delta: event.delta });
		case "thinking_delta":
			return mapAssistantMessageEvent({
				type: "thinking_delta",
				contentIndex: event.contentIndex,
				delta: event.delta,
				partial: {
					content: event.partial.content.map((content) =>
						content.type === "thinking"
							? { type: "thinking", ...(content.redacted === undefined ? {} : { redacted: content.redacted }) }
							: { type: content.type },
					),
				},
			});
		case "toolcall_delta":
			return mapAssistantMessageEvent({
				type: "toolcall_delta",
				contentIndex: event.contentIndex,
				delta: event.delta,
			});
		case "toolcall_end":
			return mapAssistantMessageEvent({
				type: "toolcall_end",
				contentIndex: event.contentIndex,
				toolCall: {
					id: event.toolCall.id,
					name: event.toolCall.name,
					arguments: toJsonValue(event.toolCall.arguments),
				},
			});
		case "done":
			return mapAssistantMessageEvent({
				type: "done",
				reason: event.reason,
				message: { timestamp: event.message.timestamp },
			});
		case "error":
			return mapAssistantMessageEvent({
				type: "error",
				reason: event.reason,
				error: {
					timestamp: event.error.timestamp,
					...(event.error.errorMessage === undefined ? {} : { errorMessage: event.error.errorMessage }),
				},
			});
	}
}

function toJsonValue(value: unknown, ancestors = new Set<object>()): JsonValue {
	if (value === null || typeof value === "boolean" || typeof value === "string") return value;
	if (typeof value === "number") {
		if (Number.isFinite(value)) return value;
		throw new TypeError("Provider tool call contains a non-finite JSON number");
	}
	if (typeof value !== "object") throw new TypeError("Provider tool call is not JSON-compatible");
	if (ancestors.has(value)) throw new TypeError("Provider tool call contains a cyclic JSON value");
	ancestors.add(value);
	try {
		if (Array.isArray(value)) return value.map((entry) => toJsonValue(entry, ancestors));
		if (Object.getPrototypeOf(value) !== Object.prototype && Object.getPrototypeOf(value) !== null) {
			throw new TypeError("Provider tool call JSON objects must be plain objects");
		}
		const result: Record<string, JsonValue> = {};
		for (const [key, entry] of Object.entries(value)) result[key] = toJsonValue(entry, ancestors);
		return result;
	} finally {
		ancestors.delete(value);
	}
}

function messageForError(error: unknown, aborted: boolean): string {
	if (aborted) return "Operation aborted";
	if (error instanceof Error && error.message.length > 0) return error.message;
	return "Provider request failed";
}

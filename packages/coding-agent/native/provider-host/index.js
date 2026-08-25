import { mapAssistantMessageEvent, parseProviderRequest, } from "@earendil-works/pi-protocol";
import Type from "typebox";
/**
 * 真实 Provider SDK 的异步执行边界。
 *
 * 每个 requestId 绑定一个独立 AbortController。取消会立即转发给 pi-ai Provider；无论是 SDK
 * 主动发送 aborted 事件、抛出异常或超时，Host 都只向 Rust Core 输出稳定的 `error` DTO。
 */
export class ProviderHost {
    modelResolver;
    contextConverter;
    stream;
    requestOptions;
    defaultTimeoutMs;
    now;
    active = new Map();
    constructor(options) {
        this.modelResolver = options.modelResolver;
        this.contextConverter = options.contextConverter ?? defaultProviderContextConverter;
        this.stream = options.stream;
        this.requestOptions = options.requestOptions ?? {};
        this.defaultTimeoutMs = options.defaultTimeoutMs;
        this.now = options.now ?? Date.now;
    }
    /** 执行并投影一次请求；同一 requestId 同时只能存在一个活动流。 */
    async *execute(input) {
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
                if (mapped)
                    yield mapped;
            }
        }
        catch (error) {
            yield this.error(messageForError(error, controller.signal.aborted));
        }
        finally {
            if (timeout !== undefined)
                clearTimeout(timeout);
            this.active.delete(request.requestId);
        }
    }
    /** 中止指定请求；不存在的 requestId 返回 false，调用方可安全重试取消。 */
    abort(requestId) {
        const controller = this.active.get(requestId);
        if (!controller)
            return false;
        controller.abort();
        return true;
    }
    /**
     * 在 stdio EOF 或进程信号到达时取消全部活动请求。
     *
     * 这是关闭边界而非错误恢复：Provider 流仍由各自的 `execute` finally 清理，并由
     * stdio 服务写入唯一的 complete，避免 Rust Core 永远保留活动 request ID。
     */
    abortAll() {
        for (const controller of this.active.values())
            controller.abort();
    }
    get activeRequestCount() {
        return this.active.size;
    }
    scheduleTimeout(controller) {
        if (this.defaultTimeoutMs === undefined)
            return undefined;
        if (!Number.isSafeInteger(this.defaultTimeoutMs) || this.defaultTimeoutMs <= 0)
            return undefined;
        return setTimeout(() => controller.abort(), this.defaultTimeoutMs);
    }
    error(message) {
        const timestamp = this.now();
        return { type: "error", messageId: `provider-${timestamp}`, content: [], timestamp, message };
    }
}
export const defaultProviderContextConverter = {
    toContext(request) {
        return {
            systemPrompt: request.systemPrompt,
            messages: request.messages.map(toAiMessage),
            tools: request.tools.map((tool) => {
                const toolWithExecutionMode = tool;
                return {
                    name: tool.name,
                    description: tool.description,
                    parameters: toolParameters(tool.inputSchema),
                    ...(toolWithExecutionMode.executionMode === undefined
                        ? {}
                        : { executionMode: toolWithExecutionMode.executionMode }),
                };
            }),
        };
    },
};
/**
 * 协议只保证 inputSchema 是 JSON 值；pi-ai 工具参数必须是 JSON Schema 对象。
 * 在 SDK 边界显式拒绝不合法形态，避免把不可执行的 schema 静默发送给 Provider。
 */
function toolParameters(inputSchema) {
    if (!isJsonObject(inputSchema))
        throw new TypeError("Provider tool inputSchema must be a JSON Schema object");
    return Type.Unsafe(inputSchema);
}
function toAiMessage(item) {
    switch (item.role) {
        case "user":
            return toAiUserMessage(item);
        case "assistant":
            return toAiAssistantMessage(item);
        case "tool":
            return toAiToolResultMessage(item);
    }
}
function toAiUserMessage(item) {
    return {
        role: "user",
        content: item.content.map((content) => content.type === "text"
            ? { type: "text", text: content.text }
            : { type: "image", data: content.data, mimeType: content.mimeType }),
        timestamp: item.timestamp,
    };
}
function toAiAssistantMessage(item) {
    const content = item.content.map((part) => part.type === "text"
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
            });
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
function toAiToolResultMessage(item) {
    return {
        role: "toolResult",
        toolCallId: item.toolCallId,
        toolName: item.toolName,
        content: item.content.map((content) => content.type === "text"
            ? { type: "text", text: content.text }
            : { type: "image", data: content.data, mimeType: content.mimeType }),
        ...(item.details === undefined ? {} : { details: item.details }),
        ...(item.usage === undefined ? {} : { usage: toAiUsage(item.usage) }),
        isError: item.isError,
        timestamp: item.timestamp,
    };
}
function toolCallArguments(input) {
    if (!isJsonObject(input))
        throw new TypeError("Provider tool call input must be a JSON object");
    return input;
}
function isJsonObject(value) {
    return value !== null && !Array.isArray(value) && typeof value === "object";
}
function toAiUsage(usage) {
    return (usage ?? {
        input: 0,
        output: 0,
        cacheRead: 0,
        cacheWrite: 0,
        totalTokens: 0,
        cost: { input: 0, output: 0, cacheRead: 0, cacheWrite: 0, total: 0 },
    });
}
function mapAiAssistantMessageEvent(event) {
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
                    content: event.partial.content.map((content) => content.type === "thinking"
                        ? { type: "thinking", ...(content.redacted === undefined ? {} : { redacted: content.redacted }) }
                        : { type: content.type }),
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
                message: terminalMessage(event.message),
            });
        case "error":
            return mapAssistantMessageEvent({
                type: "error",
                reason: event.reason,
                error: terminalMessage(event.error),
            });
    }
}
/**
 * 只在稳定终态执行一次完整内容转换。流式 partial 仍通过轻量 delta 传输，避免每个 chunk
 * 都复制整个 transcript；终态快照则保留 Provider 最终修正后的 content 与计费信息。
 */
function terminalMessage(message) {
    return {
        ...(message.responseId === undefined ? {} : { responseId: message.responseId }),
        content: message.content.map((content) => content.type === "text"
            ? { type: "text", text: content.text }
            : content.type === "thinking"
                ? {
                    type: "thinking",
                    thinking: content.thinking,
                    ...(content.redacted === undefined ? {} : { redacted: content.redacted }),
                }
                : {
                    type: "toolCall",
                    toolCallId: content.id,
                    toolName: content.name,
                    input: toJsonValue(content.arguments),
                }),
        ...(message.responseModel === undefined ? {} : { responseModel: message.responseModel }),
        usage: message.usage,
        timestamp: message.timestamp,
        ...(message.errorMessage === undefined ? {} : { errorMessage: message.errorMessage }),
    };
}
function toJsonValue(value, ancestors = new Set()) {
    if (value === null || typeof value === "boolean" || typeof value === "string")
        return value;
    if (typeof value === "number") {
        if (Number.isFinite(value))
            return value;
        throw new TypeError("Provider tool call contains a non-finite JSON number");
    }
    if (typeof value !== "object")
        throw new TypeError("Provider tool call is not JSON-compatible");
    if (ancestors.has(value))
        throw new TypeError("Provider tool call contains a cyclic JSON value");
    ancestors.add(value);
    try {
        if (Array.isArray(value))
            return value.map((entry) => toJsonValue(entry, ancestors));
        if (Object.getPrototypeOf(value) !== Object.prototype && Object.getPrototypeOf(value) !== null) {
            throw new TypeError("Provider tool call JSON objects must be plain objects");
        }
        const result = {};
        for (const [key, entry] of Object.entries(value))
            result[key] = toJsonValue(entry, ancestors);
        return result;
    }
    finally {
        ancestors.delete(value);
    }
}
function messageForError(error, aborted) {
    if (aborted)
        return "Operation aborted";
    if (error instanceof Error && error.message.length > 0)
        return error.message;
    return "Provider request failed";
}
//# sourceMappingURL=index.js.map
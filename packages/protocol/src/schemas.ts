import Type, { type Static } from "typebox";

// `follow_up` 与 SessionSnapshot 的独立 follow-up 队列改变了严格 DTO 的必填形状，
// 旧客户端无法安全解码，因此必须通过握手拒绝旧版本而非静默兼容。
export const PROTOCOL_VERSION = 2 as const;

const IdSchema = Type.String({ minLength: 1 });
const TimestampSchema = Type.Integer({ minimum: 0 });
const StrictObject = <const T extends Parameters<typeof Type.Object>[0]>(properties: T) =>
	Type.Object(properties, { additionalProperties: false });

export type JsonValue = null | boolean | number | string | JsonValue[] | { [key: string]: JsonValue };
const JsonValueRecursiveSchema = Type.Cyclic(
	{
		JsonValue: Type.Union([
			Type.Null(),
			Type.Boolean(),
			Type.Number(),
			Type.String(),
			Type.Array(Type.Ref("JsonValue")),
			Type.Record(Type.String(), Type.Ref("JsonValue")),
		]),
	},
	"JsonValue",
);
export const JsonValueSchema = Type.Unsafe<JsonValue>(JsonValueRecursiveSchema);

export const ThinkingLevelSchema = Type.Union([
	Type.Literal("off"),
	Type.Literal("minimal"),
	Type.Literal("low"),
	Type.Literal("medium"),
	Type.Literal("high"),
	Type.Literal("xhigh"),
	Type.Literal("max"),
]);
export type ThinkingLevel = Static<typeof ThinkingLevelSchema>;

/** Matches AgentHarnessPhase so adapters do not need a second phase vocabulary. */
export const SessionPhaseSchema = Type.Union([
	Type.Literal("idle"),
	Type.Literal("turn"),
	Type.Literal("compaction"),
	Type.Literal("branch_summary"),
	Type.Literal("retry"),
]);
export type SessionPhase = Static<typeof SessionPhaseSchema>;

export const ModelRefSchema = StrictObject({
	provider: IdSchema,
	id: IdSchema,
});
export type ModelRef = Static<typeof ModelRefSchema>;

export const ModelCostSchema = StrictObject({
	input: Type.Number({ minimum: 0 }),
	output: Type.Number({ minimum: 0 }),
	cacheRead: Type.Number({ minimum: 0 }),
	cacheWrite: Type.Number({ minimum: 0 }),
});

export const ModelMetadataSchema = StrictObject({
	provider: IdSchema,
	id: IdSchema,
	name: Type.String({ minLength: 1 }),
	api: IdSchema,
	reasoning: Type.Boolean(),
	input: Type.Array(Type.Union([Type.Literal("text"), Type.Literal("image")])),
	contextWindow: Type.Integer({ minimum: 1 }),
	maxTokens: Type.Integer({ minimum: 1 }),
	cost: ModelCostSchema,
	supportedThinkingLevels: Type.Array(ThinkingLevelSchema, { minItems: 1 }),
	authenticated: Type.Boolean(),
});
export type ModelMetadata = Static<typeof ModelMetadataSchema>;

export const TextContentSchema = StrictObject({
	type: Type.Literal("text"),
	text: Type.String(),
});
export const ThinkingContentSchema = StrictObject({
	type: Type.Literal("thinking"),
	thinking: Type.String(),
	redacted: Type.Optional(Type.Boolean()),
});
export const ImageContentSchema = StrictObject({
	type: Type.Literal("image"),
	data: Type.String(),
	mimeType: Type.String({ minLength: 1 }),
});
export const ToolCallContentSchema = StrictObject({
	type: Type.Literal("toolCall"),
	toolCallId: IdSchema,
	toolName: IdSchema,
	input: JsonValueSchema,
});
export const UserContentSchema = Type.Union([TextContentSchema, ImageContentSchema]);
export const AssistantContentSchema = Type.Union([TextContentSchema, ThinkingContentSchema, ToolCallContentSchema]);
export const ToolContentSchema = Type.Union([TextContentSchema, ImageContentSchema]);
export type TextContent = Static<typeof TextContentSchema>;
export type ThinkingContent = Static<typeof ThinkingContentSchema>;
export type ImageContent = Static<typeof ImageContentSchema>;
export type ToolCallContent = Static<typeof ToolCallContentSchema>;
export type AssistantContent = Static<typeof AssistantContentSchema>;

/**
 * Provider Host 与 Rust Core 间传递的已完成工具调用。
 *
 * 这不是 Provider SDK 的内部 ToolCall：字段只包含可 JSON 序列化的数据，因而可被
 * 回放 fixture、RPC 传输和 Tool Runtime 共同消费。
 */
export const ToolCallSchema = StrictObject({
	toolCallId: IdSchema,
	toolName: IdSchema,
	input: JsonValueSchema,
});
export type ToolCall = Static<typeof ToolCallSchema>;

/** Tool Runtime 执行完一次调用后返回的稳定结果。 */
export const ToolResultSchema = StrictObject({
	toolCallId: IdSchema,
	toolName: IdSchema,
	input: JsonValueSchema,
	content: Type.Array(ToolContentSchema),
	details: Type.Optional(JsonValueSchema),
	isError: Type.Boolean(),
	timestamp: TimestampSchema,
});
export type ToolResult = Static<typeof ToolResultSchema>;

export const UsageSchema = StrictObject({
	input: Type.Integer({ minimum: 0 }),
	output: Type.Integer({ minimum: 0 }),
	cacheRead: Type.Integer({ minimum: 0 }),
	cacheWrite: Type.Integer({ minimum: 0 }),
	reasoning: Type.Optional(Type.Integer({ minimum: 0 })),
	totalTokens: Type.Integer({ minimum: 0 }),
	cost: StrictObject({
		input: Type.Number({ minimum: 0 }),
		output: Type.Number({ minimum: 0 }),
		cacheRead: Type.Number({ minimum: 0 }),
		cacheWrite: Type.Number({ minimum: 0 }),
		total: Type.Number({ minimum: 0 }),
	}),
});
export type Usage = Static<typeof UsageSchema>;

export const UserTranscriptItemSchema = StrictObject({
	id: IdSchema,
	role: Type.Literal("user"),
	content: Type.Array(UserContentSchema),
	timestamp: TimestampSchema,
});
const AssistantTranscriptItemProperties = {
	id: IdSchema,
	role: Type.Literal("assistant"),
	content: Type.Array(AssistantContentSchema),
	model: ModelRefSchema,
	responseModel: Type.Optional(Type.String({ minLength: 1 })),
	usage: Type.Optional(UsageSchema),
	timestamp: TimestampSchema,
} as const;
const StreamingAssistantTranscriptItemSchema = StrictObject({
	...AssistantTranscriptItemProperties,
	status: Type.Literal("streaming"),
});
const CompleteAssistantTranscriptItemSchema = StrictObject({
	...AssistantTranscriptItemProperties,
	status: Type.Literal("complete"),
	stopReason: Type.Union([Type.Literal("stop"), Type.Literal("length"), Type.Literal("toolUse")]),
});
const ErrorAssistantTranscriptItemSchema = StrictObject({
	...AssistantTranscriptItemProperties,
	status: Type.Literal("error"),
	stopReason: Type.Literal("error"),
	errorMessage: Type.Optional(Type.String({ minLength: 1 })),
});
const AbortedAssistantTranscriptItemSchema = StrictObject({
	...AssistantTranscriptItemProperties,
	status: Type.Literal("aborted"),
	stopReason: Type.Literal("aborted"),
	errorMessage: Type.Optional(Type.String()),
});
export const AssistantTranscriptItemSchema = Type.Union([
	StreamingAssistantTranscriptItemSchema,
	CompleteAssistantTranscriptItemSchema,
	ErrorAssistantTranscriptItemSchema,
	AbortedAssistantTranscriptItemSchema,
]);
const ToolTranscriptItemProperties = {
	id: IdSchema,
	role: Type.Literal("tool"),
	toolCallId: IdSchema,
	toolName: IdSchema,
	input: JsonValueSchema,
	content: Type.Array(ToolContentSchema),
	details: Type.Optional(JsonValueSchema),
	usage: Type.Optional(UsageSchema),
	timestamp: TimestampSchema,
} as const;
const RunningToolTranscriptItemSchema = StrictObject({
	...ToolTranscriptItemProperties,
	status: Type.Literal("running"),
	isError: Type.Literal(false),
});
const CompleteToolTranscriptItemSchema = StrictObject({
	...ToolTranscriptItemProperties,
	status: Type.Literal("complete"),
	isError: Type.Literal(false),
});
const ErrorToolTranscriptItemSchema = StrictObject({
	...ToolTranscriptItemProperties,
	status: Type.Literal("error"),
	isError: Type.Literal(true),
});
export const ToolTranscriptItemSchema = Type.Union([
	RunningToolTranscriptItemSchema,
	CompleteToolTranscriptItemSchema,
	ErrorToolTranscriptItemSchema,
]);
export const TranscriptItemSchema = Type.Union([
	UserTranscriptItemSchema,
	AssistantTranscriptItemSchema,
	ToolTranscriptItemSchema,
]);
export type UserTranscriptItem = Static<typeof UserTranscriptItemSchema>;
export type AssistantTranscriptItem = Static<typeof AssistantTranscriptItemSchema>;
export type ToolTranscriptItem = Static<typeof ToolTranscriptItemSchema>;
export type TranscriptItem = Static<typeof TranscriptItemSchema>;

/**
 * 发往 Provider Host 的一次不可变请求快照。
 *
 * Host 只能依据这个 DTO 选择 Provider 并建立流；认证、SDK 选项与网络传输都留在
 * Host 进程，不能反向泄漏到 Rust Core。
 */
export const ProviderRequestSchema = StrictObject({
	requestId: IdSchema,
	model: ModelRefSchema,
	systemPrompt: Type.String(),
	messages: Type.Array(TranscriptItemSchema),
	tools: Type.Array(
		StrictObject({
			name: IdSchema,
			description: Type.String(),
			inputSchema: JsonValueSchema,
			// 与 Rust Tool Runtime 的批次调度约束对齐。省略时默认为 parallel，兼容既有请求。
			executionMode: Type.Optional(Type.Union([Type.Literal("parallel"), Type.Literal("sequential")])),
		}),
	),
});
export type ProviderRequest = Static<typeof ProviderRequestSchema>;

/**
 * Provider Host 归一化后的流事件。
 *
 * 增量只负责实时 progress；`done` / `error` 携带 Provider SDK 给出的稳定终态快照。
 * Rust Core 在终态到达时用该快照替换累计 partial，这与 TypeScript Agent Loop 调用
 * `response.result()` 后替换 partial message 的行为一致，也避免丢失 usage、responseModel
 * 或 Provider 在最后一个 chunk 才修正的内容。
 */
export const ProviderStreamEventSchema = Type.Union([
	StrictObject({ type: Type.Literal("start"), messageId: IdSchema, timestamp: TimestampSchema }),
	StrictObject({ type: Type.Literal("text_delta"), contentIndex: Type.Integer({ minimum: 0 }), delta: Type.String() }),
	StrictObject({
		type: Type.Literal("thinking_delta"),
		contentIndex: Type.Integer({ minimum: 0 }),
		delta: Type.String(),
		redacted: Type.Optional(Type.Boolean()),
	}),
	StrictObject({
		type: Type.Literal("tool_call_delta"),
		contentIndex: Type.Integer({ minimum: 0 }),
		delta: Type.String(),
	}),
	StrictObject({
		type: Type.Literal("tool_call_end"),
		contentIndex: Type.Integer({ minimum: 0 }),
		toolCall: ToolCallSchema,
	}),
	StrictObject({
		type: Type.Literal("done"),
		messageId: IdSchema,
		content: Type.Array(AssistantContentSchema),
		responseModel: Type.Optional(Type.String({ minLength: 1 })),
		usage: UsageSchema,
		timestamp: TimestampSchema,
		stopReason: Type.Union([Type.Literal("stop"), Type.Literal("length"), Type.Literal("toolUse")]),
	}),
	StrictObject({
		type: Type.Literal("error"),
		messageId: IdSchema,
		content: Type.Array(AssistantContentSchema),
		responseModel: Type.Optional(Type.String({ minLength: 1 })),
		usage: Type.Optional(UsageSchema),
		timestamp: TimestampSchema,
		message: Type.String({ minLength: 1 }),
	}),
]);
export type ProviderStreamEvent = Static<typeof ProviderStreamEventSchema>;

/**
 * Rust Core 发往 Provider Host 的独立进程消息。
 *
 * 该通道与客户端 RPC 共用 CBOR + length-prefix framing，但不复用客户端命令。每个
 * `requestId` 只能用于一次 request；abort 可重复发送，Host 必须将其视为幂等操作。
 */
export const ProviderHostRequestSchema = Type.Union([
	StrictObject({ type: Type.Literal("request"), request: ProviderRequestSchema }),
	StrictObject({ type: Type.Literal("abort"), requestId: IdSchema }),
]);
export type ProviderHostRequest = Static<typeof ProviderHostRequestSchema>;

/**
 * Provider Host 回传 Rust Core 的独立进程消息。
 *
 * 每个 request 产生零个或多个 event，并且恰好以一个 complete 收尾。Host 失败时先发送
 * 共享 `error` stream event，再发送 complete，使 Core 可为每个请求维持单一终态路径。
 */
export const ProviderHostResponseSchema = Type.Union([
	StrictObject({ type: Type.Literal("event"), requestId: IdSchema, event: ProviderStreamEventSchema }),
	StrictObject({ type: Type.Literal("complete"), requestId: IdSchema }),
]);
export type ProviderHostResponse = Static<typeof ProviderHostResponseSchema>;

/** Normalized incremental activity. Snapshots remain authoritative. */
export const TranscriptProgressSchema = Type.Union([
	StrictObject({
		type: Type.Literal("item_started"),
		item: TranscriptItemSchema,
	}),
	StrictObject({
		type: Type.Literal("assistant_delta"),
		messageId: IdSchema,
		contentIndex: Type.Integer({ minimum: 0 }),
		kind: Type.Union([Type.Literal("text"), Type.Literal("thinking"), Type.Literal("toolCall")]),
		delta: Type.String(),
	}),
	StrictObject({
		type: Type.Literal("item_updated"),
		item: Type.Union([AssistantTranscriptItemSchema, ToolTranscriptItemSchema]),
	}),
	StrictObject({
		type: Type.Literal("item_finished"),
		item: Type.Union([
			CompleteAssistantTranscriptItemSchema,
			ErrorAssistantTranscriptItemSchema,
			AbortedAssistantTranscriptItemSchema,
			CompleteToolTranscriptItemSchema,
			ErrorToolTranscriptItemSchema,
		]),
	}),
]);
export type TranscriptProgress = Static<typeof TranscriptProgressSchema>;

export const SessionMetadataSchema = StrictObject({
	id: IdSchema,
	createdAt: TimestampSchema,
	updatedAt: Type.Optional(TimestampSchema),
	parentSessionId: Type.Optional(IdSchema),
	sessionName: Type.Optional(Type.String()),
	cwd: Type.Optional(Type.String({ minLength: 1 })),
});
export const SessionSnapshotSchema = StrictObject({
	id: IdSchema,
	name: Type.Optional(Type.String()),
	cwd: Type.String({ minLength: 1 }),
	createdAt: TimestampSchema,
	updatedAt: TimestampSchema,
	phase: SessionPhaseSchema,
	model: ModelRefSchema,
	thinkingLevel: ThinkingLevelSchema,
	attached: Type.Boolean(),
	locked: Type.Boolean(),
	revision: Type.Integer({ minimum: 0 }),
	transcript: Type.Array(TranscriptItemSchema),
	queuedSteer: Type.Array(UserTranscriptItemSchema),
	queuedSteerCount: Type.Integer({ minimum: 0 }),
	// follow-up 与 steering 使用不同调度语义，必须分别投影，避免 UI 将“本回合末尾
	// 消费”的 follow-up 错当作下一次 Provider 请求前立即注入的 steering。
	queuedFollowUp: Type.Array(UserTranscriptItemSchema),
	queuedFollowUpCount: Type.Integer({ minimum: 0 }),
});
export type SessionMetadata = Static<typeof SessionMetadataSchema>;
export type SessionSnapshot = Static<typeof SessionSnapshotSchema>;

export const ServerSnapshotSchema = StrictObject({
	serverId: IdSchema,
	protocolVersion: Type.Literal(PROTOCOL_VERSION),
	revision: Type.Integer({ minimum: 0 }),
	sessions: Type.Array(SessionMetadataSchema),
	models: Type.Array(ModelMetadataSchema),
});
export type ServerSnapshot = Static<typeof ServerSnapshotSchema>;

export const ProtocolErrorCodeSchema = Type.Union([
	Type.Literal("version"),
	Type.Literal("busy"),
	Type.Literal("session_locked"),
	Type.Literal("not_found"),
	Type.Literal("invalid_request"),
	Type.Literal("not_implemented"),
	Type.Literal("internal_error"),
]);
export const ProtocolErrorSchema = StrictObject({
	code: ProtocolErrorCodeSchema,
	message: Type.String(),
	details: Type.Optional(JsonValueSchema),
});
export type ProtocolErrorCode = Static<typeof ProtocolErrorCodeSchema>;
export type ProtocolError = Static<typeof ProtocolErrorSchema>;

const PromptPayloadProperties = {
	sessionId: IdSchema,
	text: Type.String(),
} as const;

export const ListCommandSchema = StrictObject({ command: Type.Literal("list") });
export const CreateCommandSchema = StrictObject({
	command: Type.Literal("create"),
	cwd: Type.Optional(Type.String({ minLength: 1 })),
	name: Type.Optional(Type.String()),
	model: Type.Optional(ModelRefSchema),
	thinkingLevel: Type.Optional(ThinkingLevelSchema),
});
export const AttachCommandSchema = StrictObject({ command: Type.Literal("attach"), sessionId: IdSchema });
export const DetachCommandSchema = StrictObject({ command: Type.Literal("detach"), sessionId: IdSchema });
export const PromptCommandSchema = StrictObject({ command: Type.Literal("prompt"), ...PromptPayloadProperties });
export const SteerCommandSchema = StrictObject({ command: Type.Literal("steer"), ...PromptPayloadProperties });
/** 在当前 Agent 回合自然结束后投递的用户输入，不会中断现有 Provider 或工具流。 */
export const FollowUpCommandSchema = StrictObject({ command: Type.Literal("follow_up"), ...PromptPayloadProperties });
export const AbortCommandSchema = StrictObject({ command: Type.Literal("abort"), sessionId: IdSchema });
export const SetModelCommandSchema = StrictObject({
	command: Type.Literal("set_model"),
	sessionId: IdSchema,
	model: ModelRefSchema,
});
export const SetThinkingCommandSchema = StrictObject({
	command: Type.Literal("set_thinking"),
	sessionId: IdSchema,
	thinkingLevel: ThinkingLevelSchema,
});
export const CommandSchema = Type.Union([
	ListCommandSchema,
	CreateCommandSchema,
	AttachCommandSchema,
	DetachCommandSchema,
	PromptCommandSchema,
	SteerCommandSchema,
	FollowUpCommandSchema,
	AbortCommandSchema,
	SetModelCommandSchema,
	SetThinkingCommandSchema,
]);
export type Command = Static<typeof CommandSchema>;
export type CommandName = Command["command"];

export const CreateResultSchema = StrictObject({
	command: Type.Literal("create"),
	session: SessionSnapshotSchema,
});
export const AttachResultSchema = StrictObject({
	command: Type.Literal("attach"),
	session: SessionSnapshotSchema,
});
export const PromptResultSchema = StrictObject({
	command: Type.Literal("prompt"),
	session: SessionSnapshotSchema,
});
export const SteerResultSchema = StrictObject({
	command: Type.Literal("steer"),
	session: SessionSnapshotSchema,
});
export const FollowUpResultSchema = StrictObject({
	command: Type.Literal("follow_up"),
	session: SessionSnapshotSchema,
});
export const AbortResultSchema = StrictObject({
	command: Type.Literal("abort"),
	session: SessionSnapshotSchema,
});
export const SetModelResultSchema = StrictObject({
	command: Type.Literal("set_model"),
	session: SessionSnapshotSchema,
});
export const SetThinkingResultSchema = StrictObject({
	command: Type.Literal("set_thinking"),
	session: SessionSnapshotSchema,
});

export const ListResultSchema = StrictObject({
	command: Type.Literal("list"),
	sessions: Type.Array(SessionMetadataSchema),
});
export const DetachResultSchema = StrictObject({
	command: Type.Literal("detach"),
	sessionId: IdSchema,
});
export const CommandResultSchema = Type.Union([
	ListResultSchema,
	CreateResultSchema,
	AttachResultSchema,
	DetachResultSchema,
	PromptResultSchema,
	SteerResultSchema,
	FollowUpResultSchema,
	AbortResultSchema,
	SetModelResultSchema,
	SetThinkingResultSchema,
]);
export type CommandResult = Static<typeof CommandResultSchema>;

export type ResultForCommand<TCommand extends Command> = TCommand["command"] extends "list"
	? Static<typeof ListResultSchema>
	: TCommand["command"] extends "detach"
		? Static<typeof DetachResultSchema>
		: Extract<CommandResult, { command: TCommand["command"] }>;

/** Must be the first frame sent by a client. Version is intentionally an integer, not a coercible string. */
export const ClientHelloSchema = StrictObject({
	type: Type.Literal("hello"),
	version: Type.Integer({ minimum: 0 }),
});
export type ClientHello = Static<typeof ClientHelloSchema>;

export const RequestEnvelopeSchema = StrictObject({
	type: Type.Literal("request"),
	id: IdSchema,
	request: CommandSchema,
});
export type RequestEnvelope = Static<typeof RequestEnvelopeSchema>;
export const ClientMessageSchema = Type.Union([ClientHelloSchema, RequestEnvelopeSchema]);
export type ClientMessage = Static<typeof ClientMessageSchema>;

export const ServerEventSchema = Type.Union([
	StrictObject({ type: Type.Literal("server_snapshot"), snapshot: ServerSnapshotSchema }),
	StrictObject({ type: Type.Literal("session_snapshot"), snapshot: SessionSnapshotSchema }),
	StrictObject({
		type: Type.Literal("session_progress"),
		sessionId: IdSchema,
		progress: TranscriptProgressSchema,
	}),
	StrictObject({ type: Type.Literal("session_removed"), sessionId: IdSchema }),
]);
export type ServerEvent = Static<typeof ServerEventSchema>;

export const ServerHelloSchema = StrictObject({
	type: Type.Literal("hello"),
	version: Type.Literal(PROTOCOL_VERSION),
	connectionId: IdSchema,
	snapshot: ServerSnapshotSchema,
});
export const ServerHelloErrorSchema = StrictObject({
	type: Type.Literal("hello_error"),
	error: ProtocolErrorSchema,
});
export const ResponseEnvelopeSchema = Type.Union([
	StrictObject({
		type: Type.Literal("response"),
		id: IdSchema,
		ok: Type.Literal(true),
		result: CommandResultSchema,
	}),
	StrictObject({
		type: Type.Literal("response"),
		id: IdSchema,
		ok: Type.Literal(false),
		error: ProtocolErrorSchema,
	}),
]);
export const EventEnvelopeSchema = StrictObject({
	type: Type.Literal("event"),
	event: ServerEventSchema,
});
export const ServerMessageSchema = Type.Union([
	ServerHelloSchema,
	ServerHelloErrorSchema,
	ResponseEnvelopeSchema,
	EventEnvelopeSchema,
]);
export type ServerHello = Static<typeof ServerHelloSchema>;
export type ServerHelloError = Static<typeof ServerHelloErrorSchema>;
export type ResponseEnvelope = Static<typeof ResponseEnvelopeSchema>;
export type EventEnvelope = Static<typeof EventEnvelopeSchema>;
export type ServerMessage = Static<typeof ServerMessageSchema>;

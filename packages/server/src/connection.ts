import type { ClientMessageDecoder, EventEnvelope } from "@earendil-works/pi-protocol";

import type { MaybePromise } from "./types.ts";

/** An established, authorized ordered byte connection. */
export interface ByteConnection {
	readonly closed: boolean;
	send(chunk: Uint8Array): Promise<void>;
	close(finalChunk?: Uint8Array): MaybePromise<void>;
}

export interface ByteConnectionHandler {
	onData(chunk: Uint8Array): void;
	onClose(): void;
	onError(error: Error): void;
}

export type ByteConnectionAcceptor = (connection: ByteConnection) => ByteConnectionHandler;

export type ConnectionStage = "awaitingHello" | "handshaking" | "ready" | "closing" | "closed";

/**
 * 业务事件在进入传输边界前的内部表示。
 *
 * `sequence` 只能由连接边界分配：这样多个事件生产者不会竞争同一个计数器，也不能
 * 错误地复用或跳过某个连接中的事件序号。
 */
export interface UnsequencedEventEnvelope {
	readonly type: "event";
	readonly event: EventEnvelope["event"];
}

export interface ConnectionState {
	id: string;
	connection: ByteConnection;
	/** 每条 transport connection 独立编号；重连由 hello snapshot 恢复，不重放旧连接事件。 */
	nextEventSequence: number;
	decoder: ClientMessageDecoder;
	sessionIds: Set<string>;
	stage: ConnectionStage;
	disconnected: boolean;
	handshakeComplete: boolean;
	handshake?: Promise<void>;
	handshakeTimeout: NodeJS.Timeout;
}

export function isTerminalConnection(state: ConnectionState): boolean {
	return state.disconnected || state.stage === "closing" || state.stage === "closed";
}

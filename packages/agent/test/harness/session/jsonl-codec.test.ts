import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";
import {
	encodeHeader,
	encodeMutation,
	metadataFromHeader,
	parseHeader,
	parseMutation,
} from "../../../src/harness/session/jsonl/codec.ts";
import { JsonlDecodeError } from "../../../src/harness/session/jsonl/errors.ts";
import type { JsonlV4Header } from "../../../src/harness/session/jsonl/types.ts";
import type { SessionMutation } from "../../../src/harness/session/state.ts";

function expectHeaderRoundTrip(header: JsonlV4Header): void {
	const encoded = encodeHeader(header);
	expect(encoded.endsWith("\n")).toBe(true);
	expect(parseHeader(encoded.trimEnd())).toEqual({ ok: true, value: header });
}

function expectMutationRoundTrip(mutation: SessionMutation): void {
	const encoded = encodeMutation(mutation);
	expect(encoded.endsWith("\n")).toBe(true);
	expect(parseMutation(encoded.trimEnd())).toEqual({ ok: true, value: mutation });
}

/**
 * Rust 侧写出的固定 JSONL v4 样本。
 *
 * 此 fixture 由 Rust integration test 覆盖其状态投影；这里直接按 TypeScript
 * codec 逐行解码，形成不依赖运行时进程或临时目录的双向格式契约。新增字段时
 * 必须同时保证 Rust 写出仍可由 TypeScript 解码，避免只验证单向兼容。
 */
const RUST_V4_FIXTURE = fileURLToPath(
	new URL("../../../../../makima-runtime/crates/session/tests/fixtures/rust-v4-session.jsonl", import.meta.url),
);

describe("JSONL v4 codec", () => {
	it("解码 Rust 写出的 v4 fixture，并保留 header、mutation 与清除语义", () => {
		const lines = readFileSync(RUST_V4_FIXTURE, "utf8").trimEnd().split("\n");
		const header = parseHeader(lines[0]!);
		expect(header).toMatchObject({
			ok: true,
			value: {
				id: "rust-fixture-session",
				legacyParentSessionPath: "C:/sessions/legacy-parent.jsonl",
				metadata: { source: "rust-fixture", nested: { enabled: true } },
			},
		});

		const mutations = lines.slice(1).map((line) => parseMutation(line));
		expect(mutations.every((result) => result.ok)).toBe(true);
		expect(
			mutations.map((result) => {
				if (!result.ok) throw result.error;
				return result.value.kind;
			}),
		).toEqual(["entry", "entry", "lane", "record", "record", "record", "fact", "fact"]);
		expect(mutations.at(-1)).toMatchObject({
			ok: true,
			value: { kind: "fact", fact: "label", targetId: "reply", label: "keep", seq: 8 },
		});
	});

	describe("headers", () => {
		it("round trips every header field with a resolved parent", () => {
			expectHeaderRoundTrip({
				kind: "header",
				version: 4,
				id: "session",
				createdAt: 1_700_000_000_000,
				cwd: "/workspace/project",
				parentSessionId: "parent",
				metadata: { owner: "agent", nested: { enabled: true }, values: [1, null, "two"] },
			});
		});

		it("round trips an unresolved legacy parent path", () => {
			expectHeaderRoundTrip({
				kind: "header",
				version: 4,
				id: "legacy-child",
				createdAt: 1_700_000_000_001,
				cwd: "/workspace/project",
				legacyParentSessionPath: "/sessions/missing-parent.jsonl",
			});
		});

		it("projects header and filesystem fields into metadata", () => {
			const header = {
				kind: "header",
				version: 4,
				id: "session",
				createdAt: 1_700_000_000_000,
				cwd: "/workspace/project",
				legacyParentSessionPath: "/sessions/missing-parent.jsonl",
				metadata: { owner: "agent" },
			} satisfies JsonlV4Header;

			expect(metadataFromHeader(header, "/sessions/session.jsonl", 1_700_000_000_100)).toEqual({
				id: "session",
				createdAt: 1_700_000_000_000,
				cwd: "/workspace/project",
				path: "/sessions/session.jsonl",
				modifiedAt: 1_700_000_000_100,
				sourceFormat: 4,
				legacyParentSessionPath: "/sessions/missing-parent.jsonl",
				metadata: { owner: "agent" },
			});
		});
	});

	describe("mutation lines", () => {
		it("returns syntax and schema errors", () => {
			for (const [line, kind] of [
				["{", "syntax"],
				[JSON.stringify({ kind: "unknown", seq: 1 }), "schema"],
			] as const) {
				const result = parseMutation(line);
				expect(result.ok).toBe(false);
				if (result.ok) throw new Error(`Expected ${kind} decode error`);
				expect(result.error).toBeInstanceOf(JsonlDecodeError);
				expect(result.error).toMatchObject({ kind });
			}
		});

		it("round trips a lane-bound entry line", () => {
			expectMutationRoundTrip({
				kind: "entry",
				lane: "main",
				entry: {
					type: "custom",
					id: "entry-1",
					seq: 1,
					parentId: null,
					timestamp: 100,
					customType: "note",
					data: { text: "hello" },
				},
			});
		});

		it("round trips an imported entry line without a lane", () => {
			expectMutationRoundTrip({
				kind: "entry",
				entry: {
					type: "custom",
					id: "entry-1",
					seq: 1,
					parentId: null,
					timestamp: 100,
					customType: "note",
				},
			});
		});

		it("round trips a record line", () => {
			expectMutationRoundTrip({
				kind: "record",
				record: {
					type: "operation_started",
					id: "run-1",
					seq: 1,
					lane: "main",
					timestamp: 100,
					sourceLeafId: null,
					intent: { kind: "run", originalPrompt: [], initialMessages: [] },
				},
			});
		});

		it("round trips a lane line", () => {
			expectMutationRoundTrip({ kind: "lane", seq: 1, lane: "thread", leafId: "entry-1" });
		});

		it("round trips fact lines, including cleared values", () => {
			expectMutationRoundTrip({ kind: "fact", seq: 1, fact: "name", name: "Example" });
			expectMutationRoundTrip({ kind: "fact", seq: 2, fact: "name", name: undefined });
			expectMutationRoundTrip({
				kind: "fact",
				seq: 3,
				fact: "label",
				targetId: "entry-1",
				label: "checkpoint",
			});
		});

		it.each([
			{
				name: "a custom entry without customType",
				mutation: { kind: "entry", type: "custom", id: "entry", parentId: null, seq: 1, timestamp: 1 },
			},
			{
				name: "an operation_started record without intent",
				mutation: {
					kind: "record",
					type: "operation_started",
					id: "run",
					lane: "main",
					seq: 1,
					timestamp: 1,
					sourceLeafId: null,
				},
			},
			{
				name: "an operation_finished record without runId",
				mutation: {
					kind: "record",
					type: "operation_finished",
					id: "finish",
					lane: "main",
					seq: 1,
					timestamp: 1,
					outcome: "completed",
				},
			},
		])("rejects $name", ({ mutation }) => {
			expect(parseMutation(JSON.stringify(mutation))).toMatchObject({ ok: false });
		});
	});
});

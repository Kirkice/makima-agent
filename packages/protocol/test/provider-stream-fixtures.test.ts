import { readdir, readFile } from "node:fs/promises";
import { fileURLToPath } from "node:url";
import { describe, expect, test } from "vitest";
import { type ProviderStreamEvent, parseProviderStreamEvent } from "../src/index.ts";

interface ProviderStreamFixture {
	name: string;
	events: ProviderStreamEvent[];
	expected: {
		eventNames: string[];
		assistantText?: string;
		assistantStatus?: "error";
		toolCall?: {
			toolCallId: string;
			toolName: string;
			input: unknown;
		};
	};
}

const fixtureDirectory = fileURLToPath(new URL("./fixtures/provider-stream/", import.meta.url));

async function loadFixtures(): Promise<ProviderStreamFixture[]> {
	const names = (await readdir(fixtureDirectory)).filter((name) => name.endsWith(".json")).sort();
	return Promise.all(
		names.map(
			async (name) => JSON.parse(await readFile(`${fixtureDirectory}${name}`, "utf8")) as ProviderStreamFixture,
		),
	);
}

describe("shared Provider Stream fixtures", () => {
	test("contain only protocol-valid normalized events", async () => {
		const fixtures = await loadFixtures();
		expect(fixtures.map((fixture) => fixture.name)).toEqual(["provider-error", "text-multi-delta", "tool-call"]);

		for (const fixture of fixtures) {
			expect(fixture.events.map(parseProviderStreamEvent)).toEqual(fixture.events);
		}
	});
});

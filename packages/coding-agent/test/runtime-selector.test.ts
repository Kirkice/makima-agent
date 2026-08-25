import { createHash } from "node:crypto";
import { mkdirSync, mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { afterEach, describe, expect, test } from "vitest";
import {
	consumeRuntimeSelection,
	providerHostLaunchArgs,
	resolveNativeRuntimeResources,
	resolvePackageDir,
	resolveRuntimeSelection,
	runtimeFallbackRecord,
} from "../src/cli/runtime-selector.ts";

const temporaryDirectories: string[] = [];

afterEach(() => {
	for (const directory of temporaryDirectories.splice(0)) rmSync(directory, { recursive: true, force: true });
});

describe("runtime selector", () => {
	test("CLI --runtime 覆盖环境变量，并从转交参数中移除自身", () => {
		const consumed = consumeRuntimeSelection(["--provider", "openai", "--runtime=native", "-p", "hello"]);

		expect(consumed).toEqual({ selection: "native", args: ["--provider", "openai", "-p", "hello"] });
		expect(resolveRuntimeSelection(consumed.selection, "ts")).toEqual({ selection: "native" });
	});

	test("未设置时保持 TypeScript，非法环境变量生成诊断", () => {
		expect(resolveRuntimeSelection(undefined, undefined)).toEqual({ selection: "ts" });
		expect(resolveRuntimeSelection(undefined, "unsupported")).toEqual({
			selection: "ts",
			diagnostic: 'PI_RUNTIME must be native, ts, or auto; received "unsupported"',
		});
	});

	test("拒绝缺失或非法 --runtime 值", () => {
		expect(consumeRuntimeSelection(["--runtime"])).toEqual({
			args: [],
			diagnostic: "--runtime requires native, ts, or auto; received no value",
		});
		expect(consumeRuntimeSelection(["--runtime", "unsupported"])).toEqual({
			args: [],
			diagnostic: '--runtime requires native, ts, or auto; received "unsupported"',
		});
	});

	test("只从产品根目录解析 sidecar，且验证两个必需 payload", () => {
		const packageDir = mkdtempSync(join(tmpdir(), "pi-runtime-selector-"));
		temporaryDirectories.push(packageDir);
		const nativeDir = join(packageDir, "native");
		const hostDir = join(nativeDir, "provider-host");
		mkdirSync(hostDir, { recursive: true });
		const executable = join(nativeDir, "pi-runtime.exe");
		const host = join(hostDir, "main.js");
		writeFileSync(executable, "binary");
		writeFileSync(host, "host");
		writeFileSync(
			join(nativeDir, "manifest.json"),
			JSON.stringify({
				schema: "pi.native_runtime.v1",
				platform: "win32",
				executable: { path: "pi-runtime.exe", sha256: sha256("binary") },
				providerHost: { path: "provider-host/main.js", sha256: sha256("host") },
			}),
		);

		expect(resolveNativeRuntimeResources(packageDir, "win32")).toEqual({
			executable: join(nativeDir, "pi-runtime.exe"),
			providerHostEntry: join(hostDir, "main.js"),
		});
		rmSync(join(hostDir, "main.js"));
		expect(() => resolveNativeRuntimeResources(packageDir, "win32")).toThrow(
			"native Provider Host payload is unavailable",
		);
	});

	test("拒绝损坏、越界或不匹配的 manifest payload", () => {
		const packageDir = mkdtempSync(join(tmpdir(), "pi-runtime-manifest-"));
		temporaryDirectories.push(packageDir);
		const nativeDir = join(packageDir, "native");
		mkdirSync(join(nativeDir, "provider-host"), { recursive: true });
		writeFileSync(join(nativeDir, "pi-runtime.exe"), "binary");
		writeFileSync(join(nativeDir, "provider-host", "main.js"), "host");

		writeNativeManifest(nativeDir, { executable: { path: "../outside.exe", sha256: sha256("binary") } });
		expect(() => resolveNativeRuntimeResources(packageDir, "win32")).toThrow(
			"native runtime executable is unavailable",
		);

		writeNativeManifest(nativeDir, { providerHost: { path: "provider-host/main.js", sha256: sha256("changed") } });
		expect(() => resolveNativeRuntimeResources(packageDir, "win32")).toThrow(
			"native Provider Host payload checksum mismatch",
		);

		writeFileSync(join(nativeDir, "manifest.json"), "not-json");
		expect(() => resolveNativeRuntimeResources(packageDir, "win32")).toThrow(
			"native runtime manifest is unavailable or invalid",
		);
	});

	test("Node 从入口祖先查找 package root，Bun 使用可执行文件目录", () => {
		const packageDir = mkdtempSync(join(tmpdir(), "pi-package-root-"));
		temporaryDirectories.push(packageDir);
		writeFileSync(join(packageDir, "package.json"), "{}");
		const entry = join(packageDir, "dist", "cli.js");
		mkdirSync(join(packageDir, "dist"));

		expect(resolvePackageDir(entry, join(packageDir, "pi"), false)).toBe(packageDir);
		expect(resolvePackageDir(entry, join(packageDir, "bin", "pi"), true)).toBe(join(packageDir, "bin"));
	});

	test("仅 Bun archive 为 Provider Host 注入内部 child 标记", () => {
		expect(providerHostLaunchArgs(false)).toEqual([]);
		expect(providerHostLaunchArgs(true)).toEqual(["--provider-host-arg", "--provider-host-child"]);
	});

	test("fallback 记录是稳定的单行 JSON", () => {
		expect(JSON.parse(runtimeFallbackRecord("sidecar missing"))).toEqual({
			schema: "pi.runtime_fallback.v1",
			from: "native",
			to: "ts",
			stage: "bootstrap",
			reason: "sidecar missing",
		});
	});
});

function writeNativeManifest(
	nativeDir: string,
	overrides: Partial<{
		executable: { path: string; sha256: string };
		providerHost: { path: string; sha256: string };
	}> = {},
): void {
	writeFileSync(
		join(nativeDir, "manifest.json"),
		JSON.stringify({
			schema: "pi.native_runtime.v1",
			platform: "win32",
			executable: { path: "pi-runtime.exe", sha256: sha256("binary"), ...overrides.executable },
			providerHost: { path: "provider-host/main.js", sha256: sha256("host"), ...overrides.providerHost },
		}),
	);
}

function sha256(value: string): string {
	return createHash("sha256").update(value).digest("hex");
}

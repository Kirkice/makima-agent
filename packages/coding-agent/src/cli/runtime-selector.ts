import { spawn } from "node:child_process";
import { createHash } from "node:crypto";
import { existsSync, readFileSync, statSync } from "node:fs";
import { dirname, isAbsolute, join, relative, resolve } from "node:path";

/** 迁移期可选的产品运行时。 */
export type RuntimeSelection = "native" | "ts" | "auto";

/** native sidecar 进入进程前失败的统一退出码。 */
export const NATIVE_BOOTSTRAP_EXIT_CODE = 70;

/** native runtime 启动前的资源解析结果。 */
export interface NativeRuntimeResources {
	executable: string;
	providerHostEntry: string;
}

interface NativeRuntimeManifest {
	schema: "pi.native_runtime.v1";
	platform: string;
	executable: { path: string; sha256: string };
	providerHost: { path: string; sha256: string };
}

/**
 * 对命令行仅做 runtime 消费，绝不解析或改写其余产品参数。
 *
 * 这段代码位于 TypeScript 产品 runtime 之外：native 被选择后，不能为了识别参数而加载
 * AgentSession、Provider SDK 或 TUI。保留其余参数可以让 Rust CLI 自己拒绝尚未支持的能力，
 * 避免 native 路径静默降级或吞掉用户输入。
 */
export function consumeRuntimeSelection(args: readonly string[]): {
	selection?: RuntimeSelection;
	args: string[];
	diagnostic?: string;
} {
	const remaining: string[] = [];
	let selection: RuntimeSelection | undefined;

	for (let index = 0; index < args.length; index++) {
		const arg = args[index];
		if (arg !== "--runtime" && !arg.startsWith("--runtime=")) {
			remaining.push(arg);
			continue;
		}

		const value = arg === "--runtime" ? args[++index] : arg.slice("--runtime=".length);
		if (value === "native" || value === "ts" || value === "auto") {
			selection = value;
			continue;
		}
		return {
			args: remaining,
			diagnostic: `--runtime requires native, ts, or auto; received ${value === undefined ? "no value" : JSON.stringify(value)}`,
		};
	}

	return { selection, args: remaining };
}

/** CLI 参数优先于环境变量；未设置时保持 TypeScript runtime，保证迁移期间行为不变。 */
export function resolveRuntimeSelection(
	cliSelection: RuntimeSelection | undefined,
	envValue: string | undefined,
): { selection: RuntimeSelection; diagnostic?: string } {
	if (cliSelection) return { selection: cliSelection };
	if (envValue === undefined || envValue.length === 0) return { selection: "ts" };
	if (envValue === "native" || envValue === "ts" || envValue === "auto") return { selection: envValue };
	return {
		selection: "ts",
		diagnostic: `PI_RUNTIME must be native, ts, or auto; received ${JSON.stringify(envValue)}`,
	};
}

/**
 * 从已安装产品根目录定位并校验 native sidecar，不使用 cwd 或 PATH 猜测。
 *
 * manifest 将路径和内容摘要绑定：即使发布、解压或本地安全软件造成 payload 残缺，也会在 child
 * 启动前失败。这样 `auto` 仍可安全回退，`native` 则稳定返回 bootstrap exit code。
 */
export function resolveNativeRuntimeResources(packageDir: string, platform = process.platform): NativeRuntimeResources {
	const nativeDir = resolve(packageDir, "native");
	const manifest = readNativeRuntimeManifest(join(nativeDir, "manifest.json"));
	if (manifest.platform !== platform)
		throw new Error(`native runtime manifest platform mismatch: expected ${platform}, received ${manifest.platform}`);

	const executable = resourcePath(nativeDir, manifest.executable.path, "native runtime executable");
	const providerHostEntry = resourcePath(nativeDir, manifest.providerHost.path, "native Provider Host payload");
	assertResourceHash(executable, manifest.executable.sha256, "native runtime executable");
	assertResourceHash(providerHostEntry, manifest.providerHost.sha256, "native Provider Host payload");
	return { executable, providerHostEntry };
}

/** 从当前 ESM 入口向上找到 package.json；Bun binary 使用 process.execPath 所在目录。 */
export function resolvePackageDir(entryPath: string, executablePath: string, isBunBinary: boolean): string {
	if (isBunBinary) return dirname(executablePath);
	let directory = dirname(entryPath);
	while (directory !== dirname(directory)) {
		if (existsSync(join(directory, "package.json"))) return directory;
		directory = dirname(directory);
	}
	throw new Error(`cannot locate package root from ${entryPath}`);
}

/** stderr 结构化记录只描述启动前 fallback，stdout 始终属于所选产品协议。 */
export function runtimeFallbackRecord(reason: string): string {
	return JSON.stringify({ schema: "pi.runtime_fallback.v1", from: "native", to: "ts", stage: "bootstrap", reason });
}

/**
 * 启动 native sidecar 并将 signals/stdio 原样转交。此函数只在运行时资源已经验证后调用。
 * auto 的回退仅覆盖本函数调用前的资源定位；child 启动后发生的失败必须保留 native 退出状态。
 */
export async function runNativeRuntime(resources: NativeRuntimeResources, args: readonly string[]): Promise<number> {
	return await runProcess(resources.executable, [
		...args,
		"--provider-host-program",
		process.execPath,
		...providerHostLaunchArgs(process.versions.bun !== undefined),
		"--provider-host-entry",
		resources.providerHostEntry,
	]);
}

/**
 * 为 Provider Host child 补充执行器特定的受控参数。
 *
 * Node 可直接解释 manifest 中的 JavaScript entry；Bun archive 的 `process.execPath` 是 `pi`
 * 本身，故必须先以内部标记切换到嵌入的 Host entry，避免递归执行产品 CLI。entry 文件仍作为
 * 最后一个 child 参数，供 Rust 在 spawn 前验证固定 sidecar payload 的完整性。
 */
export function providerHostLaunchArgs(isBunBinary: boolean): string[] {
	return isBunBinary ? ["--provider-host-arg", "--provider-host-child"] : [];
}

/** 以独立进程运行旧 TypeScript 产品入口，避免 native launcher 导入其依赖图。 */
export async function runTypeScriptRuntime(entry: string, args: readonly string[]): Promise<number> {
	return await runProcess(process.execPath, [entry, ...args]);
}

function runProcess(program: string, args: readonly string[]): Promise<number> {
	return new Promise<number>((resolveChild, rejectChild) => {
		const child = spawn(program, args, { cwd: process.cwd(), env: process.env, stdio: "inherit" });
		child.once("error", rejectChild);
		child.once("exit", (code, signal) => {
			if (signal === "SIGINT") resolveChild(130);
			else if (signal === "SIGHUP") resolveChild(129);
			else if (signal === "SIGTERM") resolveChild(143);
			else resolveChild(code ?? 1);
		});
	});
}

function readNativeRuntimeManifest(path: string): NativeRuntimeManifest {
	let candidate: unknown;
	try {
		candidate = JSON.parse(readFileSync(path, "utf8"));
	} catch (error) {
		throw new Error(
			`native runtime manifest is unavailable or invalid: ${path}; ${error instanceof Error ? error.message : String(error)}`,
		);
	}
	if (typeof candidate !== "object" || candidate === null) {
		throw new Error(`native runtime manifest has an unsupported shape: ${path}`);
	}
	const record = candidate as Record<string, unknown>;
	if (
		record.schema !== "pi.native_runtime.v1" ||
		typeof record.platform !== "string" ||
		!isManifestResource(record.executable) ||
		!isManifestResource(record.providerHost)
	) {
		throw new Error(`native runtime manifest has an unsupported shape: ${path}`);
	}
	return {
		schema: "pi.native_runtime.v1",
		platform: record.platform,
		executable: record.executable,
		providerHost: record.providerHost,
	};
}

function isManifestResource(value: unknown): value is { path: string; sha256: string } {
	return (
		typeof value === "object" &&
		value !== null &&
		typeof (value as { path?: unknown }).path === "string" &&
		/^[a-f0-9]{64}$/.test((value as { sha256?: unknown }).sha256 as string)
	);
}

function resourcePath(nativeDir: string, relativePath: string, description: string): string {
	const path = resolve(nativeDir, relativePath);
	const pathWithinNativeDir = relative(nativeDir, path);
	// 使用 path.relative 而非目标平台分隔符判断，避免测试模拟目标平台时错误拒绝有效 payload。
	if (
		pathWithinNativeDir.length === 0 ||
		pathWithinNativeDir === ".." ||
		pathWithinNativeDir.startsWith("../") ||
		pathWithinNativeDir.startsWith("..\\") ||
		isAbsolute(pathWithinNativeDir) ||
		!isRegularFile(path)
	)
		throw new Error(`${description} is unavailable: ${path}`);
	return path;
}

function assertResourceHash(path: string, expected: string, description: string): void {
	const actual = createHash("sha256").update(readFileSync(path)).digest("hex");
	if (actual !== expected) throw new Error(`${description} checksum mismatch: ${path}`);
}

function isRegularFile(path: string): boolean {
	try {
		return statSync(path).isFile();
	} catch {
		return false;
	}
}

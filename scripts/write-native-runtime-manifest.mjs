#!/usr/bin/env node
/**
 * 为固定 native sidecar 布局生成完整性 manifest。
 *
 * 该脚本同时被 npm payload staging 与二进制 release archive 使用，确保两种发布渠道
 * 使用完全相同的 schema、相对路径约束和 SHA-256 计算规则。它只接受 native 目录内的
 * 常规文件，拒绝路径穿越和符号链接，避免把构建机上任意文件的摘要写入发布物。
 */
import { createHash } from "node:crypto";
import { lstat, readFile, writeFile } from "node:fs/promises";
import { isAbsolute, join, relative, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const schema = "pi.native_runtime.v1";
const supportedPlatforms = new Set(["darwin", "linux", "win32"]);

// Node 没有 Bun 的 import.meta.main；通过入口文件 URL 判断可使 npm staging 与 Node 测试导入
// 共享同一实现，而不会在 import 时意外写入 manifest。
if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
	const options = parseArgs(process.argv.slice(2));
	await writeNativeRuntimeManifest(options);
}

/**
 * 写入运行时选择器消费的 manifest，并返回写入对象以便发布脚本测试其稳定字段。
 *
 * 参数必须在 `nativeDir` 内引用两个常规文件；任何软链接、绝对路径或路径穿越都被拒绝。
 */
export async function writeNativeRuntimeManifest(options) {
	const nativeDir = resolve(options.nativeDir);
	if (!supportedPlatforms.has(options.platform)) throw new Error(`unsupported platform: ${options.platform}`);
	const executable = resourcePath(nativeDir, options.executable, "native runtime executable");
	const providerHost = resourcePath(nativeDir, options.providerHost, "native Provider Host payload");
	const manifest = {
		schema,
		platform: options.platform,
		executable: { path: options.executable, sha256: await sha256(executable) },
		providerHost: { path: options.providerHost, sha256: await sha256(providerHost) },
	};
	await writeFile(resolve(nativeDir, "manifest.json"), `${JSON.stringify(manifest, null, 2)}\n`);
	return manifest;
}

/**
	* 验证已生成的 sidecar manifest，供 archive 打包前后执行纯布局/完整性检查。
	*
	* 此检查不尝试执行目标平台的二进制，因此可在 Linux CI 上验证 Darwin/Windows archive；实际
	* 可执行 smoke 仍只应在与 archive 相同的平台和架构上运行。
	*/
export async function validateNativeRuntimeManifest(nativeDir, expectedPlatform) {
	const absoluteNativeDir = resolve(nativeDir);
	const manifestPath = join(absoluteNativeDir, "manifest.json");
	let manifest;
	try {
		manifest = JSON.parse(await readFile(manifestPath, "utf8"));
	} catch (error) {
		throw new Error(`native runtime manifest is unavailable or invalid: ${manifestPath}; ${error instanceof Error ? error.message : String(error)}`);
	}
	if (
		typeof manifest !== "object" ||
		manifest === null ||
		manifest.schema !== schema ||
		!supportedPlatforms.has(manifest.platform) ||
		(expectedPlatform !== undefined && manifest.platform !== expectedPlatform) ||
		!isManifestResource(manifest.executable) ||
		!isManifestResource(manifest.providerHost)
	) {
		throw new Error(`native runtime manifest has an unsupported shape: ${manifestPath}`);
	}
	for (const [description, resource] of [
		["native runtime executable", manifest.executable],
		["native Provider Host payload", manifest.providerHost],
	]) {
		const payload = resourcePath(absoluteNativeDir, resource.path, description);
		if ((await sha256(payload)) !== resource.sha256) throw new Error(`${description} checksum mismatch: ${payload}`);
	}
	return manifest;
}

function isManifestResource(value) {
	return (
		typeof value === "object" &&
		value !== null &&
		typeof value.path === "string" &&
		typeof value.sha256 === "string" &&
		/^[a-f0-9]{64}$/.test(value.sha256)
	);
}

/** 只解析发布脚本需要的四个显式参数，未知参数立即失败以避免静默拼写错误。 */
function parseArgs(args) {
	const values = new Map();
	for (let index = 0; index < args.length; index += 2) {
		const name = args[index];
		const value = args[index + 1];
		if (!name?.startsWith("--") || !value || values.has(name)) usage(`invalid or duplicate option: ${name ?? "<missing>"}`);
		values.set(name, value);
	}

	const nativeDir = required(values, "--native-dir");
	const platform = required(values, "--platform");
	const executable = required(values, "--executable");
	const providerHost = required(values, "--provider-host");
	if (values.size !== 4) usage("unknown option");
	if (!supportedPlatforms.has(platform)) usage(`unsupported platform: ${platform}`);
	return { nativeDir, platform, executable, providerHost };
}

function required(values, name) {
	const value = values.get(name);
	if (!value) usage(`missing required option: ${name}`);
	return value;
}

function usage(message) {
	throw new Error(`${message}\nUsage: write-native-runtime-manifest.mjs --native-dir <dir> --platform <darwin|linux|win32> --executable <relative-file> --provider-host <relative-file>`);
}

/** 解析并验证发布 payload 的相对常规文件路径。 */
async function assertRegularResource(path, description) {
	const metadata = await lstat(path).catch(() => undefined);
	if (!metadata?.isFile()) throw new Error(`${description} is unavailable or is not a regular file: ${path}`);
}

function resourcePath(nativeDir, resource, description) {
	if (isAbsolute(resource)) usage(`${description} must be a relative path`);
	const path = resolve(nativeDir, resource);
	const contained = relative(nativeDir, path);
	if (contained.length === 0 || contained === ".." || contained.startsWith("../") || contained.startsWith("..\\") || isAbsolute(contained)) {
		usage(`${description} escapes native directory`);
	}
	return path;
}

async function sha256(path) {
	await assertRegularResource(path, "native payload");
	return createHash("sha256").update(await readFile(path)).digest("hex");
}

#!/usr/bin/env node
/**
 * 组装 coding-agent npm 发布物中的 Rust sidecar 与 Provider Host payload。
 *
 * 运行时选择器只认固定布局：
 *   packages/coding-agent/native/pi-runtime[.exe]
 *   packages/coding-agent/native/provider-host/main.js
 *
 * 此脚本是唯一把工作区构建产物复制到该布局的边界，避免 CLI 在运行时从 cwd、PATH 或
 * 未声明的 workspace 路径猜测资源位置。
 */
import { cp, mkdir, rename, rm, stat } from "node:fs/promises";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { spawn } from "node:child_process";

const repositoryRoot = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const codingAgentDir = join(repositoryRoot, "packages", "coding-agent");
const providerHostDir = join(repositoryRoot, "packages", "provider-host");
const runtimeDir = join(repositoryRoot, "makima-runtime");
const nativeDir = join(codingAgentDir, "native");
const binaryName = process.platform === "win32" ? "pi-runtime.exe" : "pi-runtime";
const runtimeBinary = join(runtimeDir, "target", "release", binaryName);

/**
 * 返回 npm 的 JavaScript CLI 路径。
 *
 * Windows 的 `npm.cmd` 仅是命令解释器脚本，直接以 `spawn()` 启动会失败；使用当前 Node
 * 可执行文件运行 npm CLI 可避免 `shell: true` 的参数转义风险与 DEP0190 警告。
 */
async function npmCliPath() {
	const fromNpm = process.env.npm_execpath;
	if (fromNpm) {
		await assertRegularFile(fromNpm, "npm CLI");
		return fromNpm;
	}

	const bundled = join(dirname(process.execPath), "node_modules", "npm", "bin", "npm-cli.js");
	await assertRegularFile(bundled, "bundled npm CLI");
	return bundled;
}

/** 执行构建命令，并将 stderr/stdout 原样交给调用者。 */
function run(program, args, cwd, displayName = program) {
	return new Promise((resolveCommand, rejectCommand) => {
		const child = spawn(program, args, { cwd, stdio: "inherit", shell: false });
		child.once("error", rejectCommand);
		child.once("exit", (code, signal) => {
			if (code === 0) resolveCommand();
			else rejectCommand(new Error(`${displayName} ${args.join(" ")} failed with ${signal ?? `exit code ${code ?? 1}`}`));
		});
	});
}

async function assertRegularFile(path, description) {
	try {
		if ((await stat(path)).isFile()) return;
	} catch {
		// 统一在下方报出资源名称和绝对路径。
	}
	throw new Error(`${description} is unavailable: ${path}`);
}

const npmCli = await npmCliPath();
await run(process.execPath, [npmCli, "run", "build"], providerHostDir, "npm");
await run("cargo", ["build", "--release", "--package", "cli", "--bin", "pi-runtime"], runtimeDir);
await assertRegularFile(runtimeBinary, "Rust runtime binary");
await assertRegularFile(join(providerHostDir, "dist", "main.js"), "Provider Host entry");

// 临时目录 + rename 避免构建失败后留下半套 payload。
const stagingDir = `${nativeDir}.staging`;
await rm(stagingDir, { recursive: true, force: true });
await mkdir(stagingDir, { recursive: true });
const stagedRuntime = join(stagingDir, binaryName);
const stagedProviderHost = join(stagingDir, "provider-host", "main.js");
await cp(runtimeBinary, stagedRuntime);
await cp(join(providerHostDir, "dist"), join(stagingDir, "provider-host"), { recursive: true });
await run(
	process.execPath,
	[
		join(repositoryRoot, "scripts", "write-native-runtime-manifest.mjs"),
		"--native-dir",
		stagingDir,
		"--platform",
		process.platform,
		"--executable",
		binaryName,
		"--provider-host",
		"provider-host/main.js",
	],
	repositoryRoot,
	"native manifest writer",
);
await rm(nativeDir, { recursive: true, force: true });
await rename(stagingDir, nativeDir);

console.log(`Staged native runtime payload in ${nativeDir}`);

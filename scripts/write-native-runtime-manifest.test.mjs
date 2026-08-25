import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { mkdtemp, mkdir, readFile, rm, symlink, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import test from "node:test";
import { validateNativeRuntimeManifest, writeNativeRuntimeManifest } from "./write-native-runtime-manifest.mjs";

async function payloadDirectory() {
	const nativeDir = await mkdtemp(join(tmpdir(), "pi-native-manifest-"));
	await mkdir(join(nativeDir, "provider-host"));
	await writeFile(join(nativeDir, "pi-runtime"), "runtime");
	await writeFile(join(nativeDir, "provider-host", "main.js"), "provider-host");
	return nativeDir;
}

async function removeDirectory(path) {
	await rm(path, { recursive: true, force: true });
}

test("为固定 sidecar 布局写入平台绑定和内容摘要", async (context) => {
	const nativeDir = await payloadDirectory();
	context.after(() => removeDirectory(nativeDir));

	const manifest = await writeNativeRuntimeManifest({
		nativeDir,
		platform: "linux",
		executable: "pi-runtime",
		providerHost: "provider-host/main.js",
	});

	assert.deepEqual(JSON.parse(await readFile(join(nativeDir, "manifest.json"), "utf8")), manifest);
	assert.equal(manifest.schema, "pi.native_runtime.v1");
	assert.equal(manifest.executable.sha256, createHash("sha256").update("runtime").digest("hex"));
	assert.equal(manifest.providerHost.sha256, createHash("sha256").update("provider-host").digest("hex"));
	assert.deepEqual(await validateNativeRuntimeManifest(nativeDir, "linux"), manifest);
});

test("拒绝绝对路径、目录外路径和符号链接 payload", async (context) => {
	const nativeDir = await payloadDirectory();
	context.after(() => removeDirectory(nativeDir));

	await assert.rejects(
		writeNativeRuntimeManifest({
			nativeDir,
			platform: "linux",
			executable: "../outside",
			providerHost: "provider-host/main.js",
		}),
		/escapes native directory/,
	);
	await assert.rejects(
		writeNativeRuntimeManifest({
			nativeDir,
			platform: "linux",
			executable: join(nativeDir, "pi-runtime"),
			providerHost: "provider-host/main.js",
		}),
		/must be a relative path/,
	);

	try {
		await symlink(join(nativeDir, "pi-runtime"), join(nativeDir, "linked-runtime"));
	} catch (error) {
		// Windows 未开启开发者模式时创建符号链接需要管理员权限；保留代码路径覆盖，但不让
		// 操作系统权限配置掩盖 manifest 生成器的跨平台验证结果。
		if (error && typeof error === "object" && "code" in error && error.code === "EPERM") return;
		throw error;
	}
	await assert.rejects(
		writeNativeRuntimeManifest({
			nativeDir,
			platform: "linux",
			executable: "linked-runtime",
			providerHost: "provider-host/main.js",
		}),
		/not a regular file/,
	);
});

test("验证 archive payload 时拒绝平台错配和摘要变更", async (context) => {
	const nativeDir = await payloadDirectory();
	context.after(() => removeDirectory(nativeDir));
	await writeNativeRuntimeManifest({
		nativeDir,
		platform: "linux",
		executable: "pi-runtime",
		providerHost: "provider-host/main.js",
	});

	await assert.rejects(validateNativeRuntimeManifest(nativeDir, "darwin"), /unsupported shape/);
	await writeFile(join(nativeDir, "pi-runtime"), "changed runtime");
	await assert.rejects(validateNativeRuntimeManifest(nativeDir, "linux"), /checksum mismatch/);
});

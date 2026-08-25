import { copyFileSync, mkdirSync, readFileSync, rmSync } from "node:fs";
import { join } from "node:path";
import { spawnSync } from "node:child_process";

const packageRoot = join(process.cwd(), ".tmp", "package-outside-node", "node_modules", "@earendil-works", "pi-coding-agent");
const cli = join(packageRoot, "dist", "cli.js");
const manifest = join(packageRoot, "native", "manifest.json");
const backup = `${manifest}.bak`;

function run(...args) {
	const result = spawnSync(process.execPath, [cli, ...args], { encoding: "utf8" });
	return { code: result.status ?? -1, stdout: result.stdout, stderr: result.stderr };
}

for (const args of [["--runtime", "native", "--help"], ["--runtime", "native", "--version"], ["--runtime", "auto", "--help"]]) {
	const result = run(...args);
	if (result.code !== 0) throw new Error(`${args.join(" ")} failed with ${result.code}: ${result.stderr}`);
}
copyFileSync(manifest, backup);
rmSync(manifest);
try {
	const native = run("--runtime", "native", "--help");
	if (native.code !== 70) throw new Error(`missing manifest native exit was ${native.code}: ${native.stderr}`);
	const auto = run("--runtime", "auto", "--help");
	// selector 的职责止于 native bootstrap 失败后的确定性转交；TS runtime 后续可能因
	// 产品环境（例如模型数据或 API 配置）退出非零，因此这里不能把后续退出码误判为
	// selector fallback 失败。严格校验 fallback 记录和 bootstrap 阶段即可。
	if (!auto.stderr.includes('"schema":"pi.runtime_fallback.v1"')) {
		throw new Error(`missing manifest auto fallback failed: code=${auto.code}, stderr=${auto.stderr}`);
	}
	console.log(`Package-outside Node selector smoke passed; auto fallback exit code was ${auto.code}.`);
} finally {
	copyFileSync(backup, manifest);
	rmSync(backup);
}

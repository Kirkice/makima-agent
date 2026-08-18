import { existsSync, readdirSync, readFileSync, writeFileSync } from "node:fs";
import { dirname, extname, join, normalize, relative, resolve } from "node:path";
import ts from "typescript";

const sourceRoot = "packages";
const baselinePath = "plans/m1-legacy-runtime-import-baseline.json";
const ignoredDirectories = new Set([".git", "coverage", "dist", "node_modules", "examples", "test"]);
const legacyPackagePrefix = "@earendil-works/pi-agent-core";
const legacyCodingAgentModules = new Set([
	"packages/coding-agent/src/core/agent-session-runtime.ts",
	"packages/coding-agent/src/core/agent-session-services.ts",
	"packages/coding-agent/src/core/agent-session.ts",
	"packages/coding-agent/src/core/sdk.ts",
	"packages/coding-agent/src/core/session-manager.ts",
]);

/**
 * M1 的目标不是立即删除仍被生产路径使用的旧 runtime，而是阻止迁移期间继续扩大依赖面。
 *
 * 因此检查器记录当前每条遗留导入的“导入文件 + 模块说明符”基线。后续任何新组合都会失败；
 * 迁移删除既有导入不会失败。若确实需要新增例外，必须审阅并显式更新基线，而不能静默放行。
 */
function collectProductionTypescriptFiles(directory, files) {
	for (const entry of readdirSync(directory, { withFileTypes: true })) {
		const path = join(directory, entry.name);
		if (entry.isDirectory()) {
			if (!ignoredDirectories.has(entry.name)) collectProductionTypescriptFiles(path, files);
			continue;
		}
		// 只扫描实际发布的 src：测试和示例保留对旧 runtime 的基线覆盖职责，不能反过来
		// 阻塞其迁移测试；生产依赖面才是本门禁必须冻结的对象。
		if (entry.isFile() && entry.name.endsWith(".ts") && !entry.name.endsWith(".d.ts") && normalizedPath(path).includes("/src/")) {
			files.push(path);
		}
	}
}

function normalizedPath(path) {
	return normalize(path).replaceAll("\\", "/");
}

function resolveRelativeModule(sourceFile, specifier) {
	const unresolved = resolve(dirname(sourceFile), specifier);
	const candidates = [
		unresolved,
		`${unresolved}.ts`,
		join(unresolved, "index.ts"),
	];
	return candidates.find((candidate) => existsSync(candidate));
}

function isLegacyRuntimeImport(sourceFile, specifier) {
	if (specifier === legacyPackagePrefix || specifier.startsWith(`${legacyPackagePrefix}/`)) return true;
	if (!specifier.startsWith(".")) return false;

	const target = resolveRelativeModule(sourceFile, specifier);
	if (!target) return false;
	const normalizedTarget = normalizedPath(relative(".", target));
	if (legacyCodingAgentModules.has(normalizedTarget)) return true;
	return normalizedTarget.startsWith("packages/coding-agent/src/core/tools/");
}

function collectModuleSpecifiers(sourceFile, filePath) {
	const specifiers = [];

	function addSpecifier(node) {
		if (ts.isStringLiteralLike(node) && isLegacyRuntimeImport(filePath, node.text)) {
			const { line, character } = sourceFile.getLineAndCharacterOfPosition(node.getStart(sourceFile));
			specifiers.push({
				file: normalizedPath(filePath),
				line: line + 1,
				column: character + 1,
				specifier: node.text,
			});
		}
	}

	function visit(node) {
		if (ts.isImportDeclaration(node) || ts.isExportDeclaration(node)) {
			if (node.moduleSpecifier) addSpecifier(node.moduleSpecifier);
		} else if (
			ts.isCallExpression(node) &&
			node.expression.kind === ts.SyntaxKind.ImportKeyword &&
			node.arguments[0]
		) {
			addSpecifier(node.arguments[0]);
		} else if (ts.isImportTypeNode(node) && ts.isLiteralTypeNode(node.argument)) {
			addSpecifier(node.argument.literal);
		}
		ts.forEachChild(node, visit);
	}

	visit(sourceFile);
	return specifiers;
}

function importKey(entry) {
	return `${entry.file}\u0000${entry.specifier}`;
}

function readBaseline() {
	if (!existsSync(baselinePath)) return [];
	const parsed = JSON.parse(readFileSync(baselinePath, "utf8"));
	if (!Array.isArray(parsed.allowedImports) || !parsed.allowedImports.every((entry) => typeof entry === "string")) {
		throw new Error(`${baselinePath} must contain an allowedImports string array`);
	}
	return parsed.allowedImports;
}

const files = [];
collectProductionTypescriptFiles(sourceRoot, files);
const currentImports = files
	.sort()
	.flatMap((file) => {
		const text = readFileSync(file, "utf8");
		return collectModuleSpecifiers(ts.createSourceFile(file, text, ts.ScriptTarget.Latest, true), file);
	});
const currentKeys = [...new Set(currentImports.map(importKey))].sort();

if (process.argv.includes("--update-baseline")) {
	const baseline = {
		// 此命令仅在人工审阅 current import 后用于建立或有意识地更新迁移基线。
		// CI 默认路径不带此参数，始终只验证而不修改仓库文件。
		allowedImports: currentKeys,
	};
	writeFileSync(baselinePath, `${JSON.stringify(baseline, null, "\t")}\n`);
	console.log(`Updated ${baselinePath} with ${currentKeys.length} legacy runtime imports.`);
	process.exit(0);
}

const allowedImports = new Set(readBaseline());
const failures = currentImports.filter((entry) => !allowedImports.has(importKey(entry)));

if (failures.length > 0) {
	console.error("New production imports of the TypeScript runtime scheduled for removal are not allowed:");
	for (const failure of failures) {
		console.error(`  ${failure.file}:${failure.line}:${failure.column}: ${failure.specifier}`);
	}
	console.error(`Review the migration design before deliberately updating ${baselinePath}.`);
	process.exit(1);
}

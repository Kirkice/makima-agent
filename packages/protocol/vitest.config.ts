import { fileURLToPath } from "node:url";
import { defineConfig } from "vitest/config";

const packageRoot = fileURLToPath(new URL(".", import.meta.url));

export default defineConfig({
	root: packageRoot,
	resolve: {
		alias: {
			// Windows 可能以不同盘符大小写解析 CLI 与测试 import。统一到根依赖中的
			// Vitest 实例，避免测试注册到另一个运行上下文。
			vitest: fileURLToPath(new URL("../../node_modules/vitest/dist/index.js", import.meta.url)),
		},
	},
	test: {
		globals: true,
		environment: "node",
		reporters: process.env.GITHUB_ACTIONS ? ["dot", "github-actions"] : ["dot"],
	},
});

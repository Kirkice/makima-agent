import { fileURLToPath } from "node:url";
import { defineConfig } from "vitest/config";

const packageRoot = fileURLToPath(new URL(".", import.meta.url));

export default defineConfig({
	// 包级 root 将测试发现范围限制在 Provider Host 内。
	root: packageRoot,
	resolve: {
		alias: {
			// Windows 上 npm 的 cwd 与 Node 包解析可能返回大小写不同的盘符。Vite 按路径
			// 字符串缓存模块，因此必须让测试 import 与 CLI 使用同一个 Vitest 实例。
			vitest: fileURLToPath(new URL("../../node_modules/vitest/dist/index.js", import.meta.url)),
			"@earendil-works/pi-ai/compat": fileURLToPath(new URL("./test/compat-stub.ts", import.meta.url)),
			"@earendil-works/pi-ai": fileURLToPath(new URL("../ai/src/index.ts", import.meta.url)),
			"@earendil-works/pi-protocol": fileURLToPath(new URL("../protocol/src/index.ts", import.meta.url)),
		},
	},
	test: {
		globals: true,
		environment: "node",
		reporters: process.env.GITHUB_ACTIONS ? ["dot", "github-actions"] : ["dot"],
	},
});

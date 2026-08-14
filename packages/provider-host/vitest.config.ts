import { fileURLToPath } from "node:url";
import { defineConfig } from "vitest/config";

export default defineConfig({
	resolve: {
		alias: {
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

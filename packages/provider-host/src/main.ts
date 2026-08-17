#!/usr/bin/env node
import { streamSimple } from "@earendil-works/pi-ai/compat";
import { builtinModels } from "@earendil-works/pi-ai/providers/all";
import type { ModelRef } from "@earendil-works/pi-protocol";
import { ProviderHost } from "./index.ts";
import { runProviderHostStdio } from "./stdio.ts";

const models = builtinModels();

const host = new ProviderHost({
	// 具体 Provider 注册与模型数据只属于可执行生产入口，核心 Host 保持为可注入边界。
	stream: streamSimple,
	modelResolver: {
		resolve(reference: ModelRef) {
			const model = models.getModel(reference.provider, reference.id);
			if (!model) {
				throw new Error(`Unknown Provider Host model: ${reference.provider}/${reference.id}`);
			}
			return model;
		},
	},
});

runProviderHostStdio(host);

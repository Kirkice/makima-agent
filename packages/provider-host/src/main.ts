#!/usr/bin/env node
import { builtinModels } from "@earendil-works/pi-ai/providers/all";
import type { ModelRef } from "@earendil-works/pi-protocol";
import { ProviderHost } from "./index.ts";
import { runProviderHostStdio } from "./stdio.ts";

const models = builtinModels();

const host = new ProviderHost({
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
